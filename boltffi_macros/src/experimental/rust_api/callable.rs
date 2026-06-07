use boltffi_ast::{BaseTrait, FunctionDef, ParameterDef, ParameterPassing, ReturnDef, TypeExpr};
use boltffi_binding::{HandlePresence, HandleTarget, Primitive, Receive};
use syn::{Ident, Type, parse_str};

use super::{Closure, DecodeTarget, TypeTokens};
use crate::experimental::error::Error;

#[derive(Clone, Copy)]
pub struct Callable<'a> {
    parameters: &'a [ParameterDef],
    returns: &'a ReturnDef,
}

impl<'a> Callable<'a> {
    pub fn function(function: &'a FunctionDef) -> Self {
        Self {
            parameters: &function.parameters,
            returns: &function.returns,
        }
    }

    pub fn parameters(self) -> &'a [ParameterDef] {
        self.parameters
    }

    pub fn returns(self) -> Return<'a> {
        Return::new(self.returns)
    }
}

#[derive(Clone, Copy)]
pub struct Parameter<'a> {
    definition: &'a ParameterDef,
}

impl<'a> Parameter<'a> {
    pub fn new(definition: &'a ParameterDef) -> Self {
        Self { definition }
    }

    pub fn ident(self) -> Result<Ident, Error> {
        parse_str(self.definition.name.spelling()).map_err(|_| {
            Error::SourceSyntaxMismatch("source parameter name is not a Rust identifier")
        })
    }

    pub fn written_type(self) -> Result<Type, Error> {
        TypeTokens::new(&self.definition.type_expr).map(TypeTokens::into_type)
    }

    pub fn value_type(self, receive: Receive) -> Result<Type, Error> {
        DecodeTarget::new(self.definition.passing, receive, &self.definition.type_expr)
            .map(|target| target.parameter().clone())
    }

    pub fn decode_target(self, receive: Receive) -> Result<DecodeTarget<'a>, Error> {
        DecodeTarget::new(self.definition.passing, receive, &self.definition.type_expr)
    }

    pub fn closure(self, presence: HandlePresence) -> Result<Closure<'a>, Error> {
        Closure::new(&self.definition.type_expr, presence)
    }

    pub fn handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        Handle::new(&self.definition.type_expr).matches(target, presence)
    }

    pub fn class_handle(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
        receive: Receive,
    ) -> Result<ClassHandle, Error> {
        self.handle(target, presence)?;
        let type_expr = match presence {
            HandlePresence::Required => &self.definition.type_expr,
            HandlePresence::Nullable => option_inner(&self.definition.type_expr)?,
            _ => return Err(Error::UnsupportedExpansion("unknown class handle presence")),
        };
        let ty = match (self.definition.passing, receive) {
            (ParameterPassing::Value, Receive::ByValue)
            | (ParameterPassing::Ref, Receive::ByRef)
            | (ParameterPassing::RefMut, Receive::ByMutRef) => {
                TypeTokens::new(type_expr)?.into_type()
            }
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "source class handle passing does not match binding receive mode",
                ));
            }
        };
        Ok(ClassHandle { ty, presence })
    }

    pub fn callback_object(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<CallbackObject, Error> {
        self.handle(target, presence)?;
        let type_expr = match presence {
            HandlePresence::Required => &self.definition.type_expr,
            HandlePresence::Nullable => option_inner(&self.definition.type_expr)?,
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown callback handle presence",
                ));
            }
        };
        CallbackObject::new(presence, type_expr)
    }

    pub fn scalar_option(self, primitive: Primitive) -> Result<(), Error> {
        let TypeExpr::Option(inner) = &self.definition.type_expr else {
            return Err(Error::SourceSyntaxMismatch(
                "source parameter is not an optional scalar",
            ));
        };
        let TypeExpr::Primitive(source) = inner.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source optional parameter is not scalar",
            ));
        };
        (Primitive::from(*source) == primitive)
            .then_some(())
            .ok_or(Error::SourceSyntaxMismatch(
                "source optional scalar does not match binding primitive",
            ))
    }

    pub fn direct_vec(self) -> Result<(), Error> {
        match &self.definition.type_expr {
            TypeExpr::Vec(_) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter is not a direct vector",
            )),
        }
    }

    pub fn direct_vec_element_type(self) -> Result<Type, Error> {
        self.direct_vec()?;
        let TypeExpr::Vec(element) = &self.definition.type_expr else {
            return Err(Error::SourceSyntaxMismatch(
                "source direct-vector parameter is missing element type",
            ));
        };
        TypeTokens::new(element).map(TypeTokens::into_type)
    }
}

pub struct ClassHandle {
    ty: Type,
    presence: HandlePresence,
}

impl ClassHandle {
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    pub const fn presence(&self) -> HandlePresence {
        self.presence
    }
}

#[derive(Clone, Copy)]
pub enum CallbackCarrier {
    BoxedDyn,
    ArcDyn,
}

impl CallbackCarrier {
    fn from_type_expr(type_expr: &TypeExpr) -> Result<Self, Error> {
        match type_expr {
            TypeExpr::Boxed(inner) => match inner.as_ref() {
                TypeExpr::Dyn(bounds) if matches!(&bounds.base, BaseTrait::Named { .. }) => {
                    Ok(Self::BoxedDyn)
                }
                _ => Err(Error::SourceSyntaxMismatch(
                    "source callback handle is not a boxed trait object",
                )),
            },
            TypeExpr::Arc(inner) => match inner.as_ref() {
                TypeExpr::Dyn(bounds) if matches!(&bounds.base, BaseTrait::Named { .. }) => {
                    Ok(Self::ArcDyn)
                }
                _ => Err(Error::SourceSyntaxMismatch(
                    "source callback handle is not an Arc trait object",
                )),
            },
            TypeExpr::ImplTrait(bounds) if matches!(&bounds.base, BaseTrait::Named { .. }) => {
                Err(Error::UnsupportedExpansion("impl-trait callback handles"))
            }
            _ => Err(Error::SourceSyntaxMismatch(
                "source type is not a callback handle",
            )),
        }
    }
}

pub struct CallbackObject {
    value: Type,
    object: Type,
    form: CallbackCarrier,
    presence: HandlePresence,
}

impl CallbackObject {
    fn new(presence: HandlePresence, type_expr: &TypeExpr) -> Result<Self, Error> {
        let form = CallbackCarrier::from_type_expr(type_expr)?;
        let object = callback_object_inner(type_expr).ok_or(Error::SourceSyntaxMismatch(
            "source callback handle is not a trait object container",
        ))?;
        Ok(Self {
            value: TypeTokens::new(type_expr)?.into_type(),
            object: TypeTokens::new(object)?.into_type(),
            form,
            presence,
        })
    }

    pub fn value(&self) -> &Type {
        &self.value
    }

    pub fn object(&self) -> &Type {
        &self.object
    }

    pub const fn form(&self) -> CallbackCarrier {
        self.form
    }

    pub const fn presence(&self) -> HandlePresence {
        self.presence
    }
}

pub enum HandleReturn {
    Class,
    Callback(CallbackReturn),
}

pub struct CallbackReturn {
    form: CallbackCarrier,
    presence: HandlePresence,
}

impl CallbackReturn {
    fn new(presence: HandlePresence, type_expr: &TypeExpr) -> Result<Self, Error> {
        Ok(Self {
            form: CallbackCarrier::from_type_expr(type_expr)?,
            presence,
        })
    }

    pub const fn form(&self) -> CallbackCarrier {
        self.form
    }

    pub const fn presence(&self) -> HandlePresence {
        self.presence
    }
}

#[derive(Clone, Copy)]
pub struct Return<'a> {
    definition: &'a ReturnDef,
}

impl<'a> Return<'a> {
    pub fn new(definition: &'a ReturnDef) -> Self {
        Self { definition }
    }

    pub fn written_type(self) -> Result<Option<Type>, Error> {
        match self.definition {
            ReturnDef::Void => Ok(None),
            ReturnDef::Value(type_expr) => TypeTokens::new(type_expr)
                .map(TypeTokens::into_type)
                .map(Some),
        }
    }

    pub fn value_type(self) -> Result<&'a TypeExpr, Error> {
        match self.definition {
            ReturnDef::Void => Err(Error::SourceSyntaxMismatch(
                "source return does not have a value type",
            )),
            ReturnDef::Value(type_expr) => Ok(type_expr),
        }
    }

    pub fn closure(self, presence: HandlePresence) -> Result<Closure<'a>, Error> {
        let ReturnDef::Value(type_expr) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an inline closure",
            ));
        };
        Closure::new(type_expr, presence)
    }

    pub fn handle_return(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<HandleReturn, Error> {
        let ReturnDef::Value(value) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not a handle value",
            ));
        };
        Handle::new(value).return_shape(target, presence)
    }

    pub fn scalar_option(self, primitive: Primitive) -> Result<(), Error> {
        let ReturnDef::Value(TypeExpr::Option(inner)) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an optional scalar",
            ));
        };
        let TypeExpr::Primitive(source) = inner.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "source optional return is not scalar",
            ));
        };
        (Primitive::from(*source) == primitive)
            .then_some(())
            .ok_or(Error::SourceSyntaxMismatch(
                "source optional scalar does not match binding primitive",
            ))
    }

    pub fn direct_vec(self) -> Result<(), Error> {
        match self.definition {
            ReturnDef::Value(TypeExpr::Vec(_)) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source return is not a direct vector",
            )),
        }
    }

    pub fn fallible(self) -> Result<Fallible<'a>, Error> {
        let ReturnDef::Value(TypeExpr::Result { ok, err }) = self.definition else {
            return Err(Error::SourceSyntaxMismatch("source return is not a Result"));
        };
        Ok(Fallible { ok, err })
    }
}

#[derive(Clone, Copy)]
pub struct Fallible<'a> {
    ok: &'a TypeExpr,
    err: &'a TypeExpr,
}

impl<'a> Fallible<'a> {
    pub fn ok(self) -> &'a TypeExpr {
        self.ok
    }

    pub fn error(self) -> &'a TypeExpr {
        self.err
    }

    pub fn ok_written_type(self) -> Result<Type, Error> {
        TypeTokens::new(self.ok).map(TypeTokens::into_type)
    }

    pub fn error_written_type(self) -> Result<Type, Error> {
        TypeTokens::new(self.err).map(TypeTokens::into_type)
    }

    pub fn ok_closure(self, presence: HandlePresence) -> Result<Closure<'a>, Error> {
        Closure::new(self.ok, presence)
    }

    pub fn ok_handle_return(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<HandleReturn, Error> {
        Handle::new(self.ok).return_shape(target, presence)
    }
}

struct Handle<'a> {
    source: &'a TypeExpr,
}

impl<'a> Handle<'a> {
    const fn new(source: &'a TypeExpr) -> Self {
        Self { source }
    }

    fn matches(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        match presence {
            HandlePresence::Required => required_handle_matches(self.source, target),
            HandlePresence::Nullable => {
                option_inner(self.source).and_then(|inner| required_handle_matches(inner, target))
            }
            _ => Err(Error::UnsupportedExpansion("unknown handle presence")),
        }
    }

    fn return_shape(
        self,
        target: &HandleTarget,
        presence: HandlePresence,
    ) -> Result<HandleReturn, Error> {
        match target {
            HandleTarget::Class(_) => self.matches(target, presence).map(|()| HandleReturn::Class),
            HandleTarget::Callback(_) => {
                let type_expr = match presence {
                    HandlePresence::Required => self.source,
                    HandlePresence::Nullable => option_inner(self.source)?,
                    _ => return Err(Error::UnsupportedExpansion("unknown handle presence")),
                };
                self.matches(target, presence)?;
                CallbackReturn::new(presence, type_expr).map(HandleReturn::Callback)
            }
            _ => Err(Error::UnsupportedExpansion("unknown handle return target")),
        }
    }
}

fn required_handle_matches(source: &TypeExpr, target: &HandleTarget) -> Result<(), Error> {
    match (source, target) {
        (TypeExpr::Class { .. }, HandleTarget::Class(_)) => Ok(()),
        (TypeExpr::ImplTrait(bounds), HandleTarget::Callback(_))
            if matches!(&bounds.base, BaseTrait::Named { .. }) =>
        {
            Ok(())
        }
        (TypeExpr::Boxed(_) | TypeExpr::Arc(_), HandleTarget::Callback(_))
            if callback_object_inner(source).is_some() =>
        {
            Ok(())
        }
        _ => Err(Error::SourceSyntaxMismatch(
            "source handle type does not match binding handle target",
        )),
    }
}

fn callback_object_inner(source: &TypeExpr) -> Option<&TypeExpr> {
    match source {
        TypeExpr::Boxed(inner) | TypeExpr::Arc(inner)
            if matches!(
                inner.as_ref(),
                TypeExpr::Dyn(bounds) if matches!(&bounds.base, BaseTrait::Named { .. })
            ) =>
        {
            Some(inner)
        }
        _ => None,
    }
}

fn option_inner(type_expr: &TypeExpr) -> Result<&TypeExpr, Error> {
    match type_expr {
        TypeExpr::Option(inner) => Ok(inner),
        _ => Err(Error::SourceSyntaxMismatch("source type is not optional")),
    }
}

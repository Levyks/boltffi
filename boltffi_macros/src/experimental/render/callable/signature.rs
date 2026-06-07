use boltffi_ast::{
    ConstExpr, FnSig, FunctionDef, GenericArgument, Literal, MapKind, ParameterDef,
    ParameterPassing, Path, PathRoot, ReturnDef, TraitBound, TypeExpr,
};
use boltffi_binding::{HandlePresence, HandleTarget, Primitive, Receive};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type, parse_str, parse2};

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
        rust_type(&self.definition.type_expr)
    }

    pub fn value_type(self, receive: Receive) -> Result<Type, Error> {
        match (self.definition.passing, receive) {
            (ParameterPassing::Value, Receive::ByValue)
            | (ParameterPassing::Ref, Receive::ByRef)
            | (ParameterPassing::RefMut, Receive::ByMutRef) => {
                parameter_type(self.definition.passing, &self.definition.type_expr)
            }
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter passing does not match binding receive mode",
            )),
        }
    }

    pub fn closure(self, presence: HandlePresence) -> Result<&'a FnSig, Error> {
        closure_signature(&self.definition.type_expr, presence)
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
            | (ParameterPassing::RefMut, Receive::ByMutRef) => rust_type(type_expr)?,
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
        rust_type(element)
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

pub struct CallbackObject {
    value: Type,
    object: Type,
    form: CallbackCarrier,
    presence: HandlePresence,
}

impl CallbackObject {
    fn new(presence: HandlePresence, type_expr: &TypeExpr) -> Result<Self, Error> {
        match type_expr {
            TypeExpr::Boxed(inner) => match inner.as_ref() {
                TypeExpr::Dyn(TraitBound::Trait { .. }) => Ok(Self {
                    value: rust_type(type_expr)?,
                    object: rust_type(inner)?,
                    form: CallbackCarrier::BoxedDyn,
                    presence,
                }),
                _ => Err(Error::SourceSyntaxMismatch(
                    "source callback parameter is not a boxed trait object",
                )),
            },
            TypeExpr::Arc(inner) => match inner.as_ref() {
                TypeExpr::Dyn(TraitBound::Trait { .. }) => Ok(Self {
                    value: rust_type(type_expr)?,
                    object: rust_type(inner)?,
                    form: CallbackCarrier::ArcDyn,
                    presence,
                }),
                _ => Err(Error::SourceSyntaxMismatch(
                    "source callback parameter is not an Arc trait object",
                )),
            },
            TypeExpr::ImplTrait(TraitBound::Trait { .. }) => Err(Error::UnsupportedExpansion(
                "impl-trait callback handle parameters",
            )),
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter is not a callback handle",
            )),
        }
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
            ReturnDef::Value(type_expr) => rust_type(type_expr).map(Some),
        }
    }

    pub fn closure(self, presence: HandlePresence) -> Result<&'a FnSig, Error> {
        let ReturnDef::Value(type_expr) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an inline closure",
            ));
        };
        closure_signature(type_expr, presence)
    }

    pub fn handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        let ReturnDef::Value(value) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not a handle value",
            ));
        };
        Handle::new(value).matches(target, presence)
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
        rust_type(self.ok)
    }

    pub fn error_written_type(self) -> Result<Type, Error> {
        rust_type(self.err)
    }

    pub fn ok_closure(self, presence: HandlePresence) -> Result<&'a FnSig, Error> {
        closure_signature(self.ok, presence)
    }

    pub fn ok_handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        Handle::new(self.ok).matches(target, presence)
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
}

fn required_handle_matches(source: &TypeExpr, target: &HandleTarget) -> Result<(), Error> {
    match (source, target) {
        (TypeExpr::Class { .. }, HandleTarget::Class(_))
        | (TypeExpr::ImplTrait(TraitBound::Trait { .. }), HandleTarget::Callback(_)) => Ok(()),
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
            if matches!(inner.as_ref(), TypeExpr::Dyn(TraitBound::Trait { .. })) =>
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

pub(in crate::experimental::render) fn closure_signature(
    type_expr: &TypeExpr,
    presence: HandlePresence,
) -> Result<&FnSig, Error> {
    let source = match presence {
        HandlePresence::Required => type_expr,
        HandlePresence::Nullable => option_inner(type_expr)?,
        _ => return Err(Error::UnsupportedExpansion("unknown closure presence")),
    };
    match source {
        TypeExpr::FnPtr(signature) => Ok(signature),
        TypeExpr::ImplTrait(TraitBound::Fn(function_trait)) => Ok(&function_trait.signature),
        TypeExpr::Boxed(inner) => match inner.as_ref() {
            TypeExpr::Dyn(TraitBound::Fn(function_trait)) => Ok(&function_trait.signature),
            _ => Err(Error::SourceSyntaxMismatch(
                "source closure type is not a boxed closure trait object",
            )),
        },
        _ => Err(Error::SourceSyntaxMismatch(
            "source type is not an inline closure",
        )),
    }
}

pub(in crate::experimental::render) fn rust_type(type_expr: &TypeExpr) -> Result<Type, Error> {
    parse2(rust_type_tokens(type_expr)?)
        .map_err(|_| Error::SourceSyntaxMismatch("source type expression is not a Rust type"))
}

fn parameter_type(passing: ParameterPassing, type_expr: &TypeExpr) -> Result<Type, Error> {
    let type_tokens = rust_type_tokens(type_expr)?;
    let tokens = match passing {
        ParameterPassing::Value => type_tokens,
        ParameterPassing::Ref => quote! { &#type_tokens },
        ParameterPassing::RefMut => quote! { &mut #type_tokens },
    };
    parse2(tokens)
        .map_err(|_| Error::SourceSyntaxMismatch("source parameter type is not Rust syntax"))
}

fn rust_type_tokens(type_expr: &TypeExpr) -> Result<TokenStream, Error> {
    Ok(match type_expr {
        TypeExpr::Primitive(primitive) => {
            let primitive_type: Type = parse_str(primitive.rust_name())
                .map_err(|_| Error::SourceSyntaxMismatch("primitive type is not Rust syntax"))?;
            quote! { #primitive_type }
        }
        TypeExpr::Unit => quote! { () },
        TypeExpr::String => quote! { String },
        TypeExpr::Str => quote! { str },
        TypeExpr::Record { path, .. }
        | TypeExpr::Enum { path, .. }
        | TypeExpr::Class { path, .. }
        | TypeExpr::Custom { path, .. } => path_tokens(path)?,
        TypeExpr::Dyn(bound) => {
            let bound = trait_bound_tokens(bound)?;
            quote! { dyn #bound }
        }
        TypeExpr::ImplTrait(bound) => {
            let bound = trait_bound_tokens(bound)?;
            quote! { impl #bound }
        }
        TypeExpr::Boxed(inner) => {
            let inner = rust_type_tokens(inner)?;
            quote! { Box<#inner> }
        }
        TypeExpr::Arc(inner) => {
            let inner = rust_type_tokens(inner)?;
            quote! { ::std::sync::Arc<#inner> }
        }
        TypeExpr::FnPtr(signature) => fn_pointer_tokens(signature)?,
        TypeExpr::Vec(element) => {
            let element = rust_type_tokens(element)?;
            quote! { Vec<#element> }
        }
        TypeExpr::Slice(element) => {
            let element = rust_type_tokens(element)?;
            quote! { [#element] }
        }
        TypeExpr::Option(inner) => {
            let inner = rust_type_tokens(inner)?;
            quote! { Option<#inner> }
        }
        TypeExpr::Result { ok, err } => {
            let ok = rust_type_tokens(ok)?;
            let err = rust_type_tokens(err)?;
            quote! { Result<#ok, #err> }
        }
        TypeExpr::Tuple(elements) => tuple_tokens(elements)?,
        TypeExpr::Map { kind, key, value } => {
            let key = rust_type_tokens(key)?;
            let value = rust_type_tokens(value)?;
            match kind {
                MapKind::Hash => quote! { ::std::collections::HashMap<#key, #value> },
                MapKind::BTree => quote! { ::std::collections::BTreeMap<#key, #value> },
            }
        }
        TypeExpr::SelfType => quote! { Self },
        TypeExpr::Parameter(parameter) => {
            let ident: Ident = parse_str(&parameter.name)
                .map_err(|_| Error::SourceSyntaxMismatch("type parameter is not an identifier"))?;
            quote! { #ident }
        }
    })
}

fn trait_bound_tokens(bound: &TraitBound) -> Result<TokenStream, Error> {
    Ok(match bound {
        TraitBound::Trait { path, .. } => path_tokens(path)?,
        TraitBound::Fn(function_trait) => {
            let name = function_trait.kind.as_ref();
            let trait_ident: Ident = parse_str(name).map_err(|_| {
                Error::SourceSyntaxMismatch("closure trait name is not an identifier")
            })?;
            let parameters = function_trait
                .signature
                .parameters
                .iter()
                .map(rust_type_tokens)
                .collect::<Result<Vec<_>, _>>()?;
            match &function_trait.signature.returns {
                ReturnDef::Void => quote! { #trait_ident(#(#parameters),*) },
                ReturnDef::Value(return_type) => {
                    let return_type = rust_type_tokens(return_type)?;
                    quote! { #trait_ident(#(#parameters),*) -> #return_type }
                }
            }
        }
    })
}

fn fn_pointer_tokens(signature: &FnSig) -> Result<TokenStream, Error> {
    let parameters = signature
        .parameters
        .iter()
        .map(rust_type_tokens)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match &signature.returns {
        ReturnDef::Void => quote! { fn(#(#parameters),*) },
        ReturnDef::Value(return_type) => {
            let return_type = rust_type_tokens(return_type)?;
            quote! { fn(#(#parameters),*) -> #return_type }
        }
    })
}

fn tuple_tokens(elements: &[TypeExpr]) -> Result<TokenStream, Error> {
    let elements = elements
        .iter()
        .map(rust_type_tokens)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match elements.as_slice() {
        [] => quote! { () },
        [element] => quote! { (#element,) },
        _ => quote! { (#(#elements),*) },
    })
}

fn path_tokens(path: &Path) -> Result<TokenStream, Error> {
    let path = path_string(path)?;
    parse_str::<Type>(&path)
        .map(|ty| quote!(#ty))
        .map_err(|_| Error::SourceSyntaxMismatch("source path is not a Rust type path"))
}

fn path_string(path: &Path) -> Result<String, Error> {
    let prefix = match path.root {
        PathRoot::Relative => String::new(),
        PathRoot::Crate => "crate::".to_owned(),
        PathRoot::Self_ => "self::".to_owned(),
        PathRoot::Super(count) => {
            std::iter::repeat_n("super", count.get())
                .collect::<Vec<_>>()
                .join("::")
                + "::"
        }
        PathRoot::Absolute => "::".to_owned(),
    };
    let segments = path
        .segments
        .iter()
        .map(|segment| {
            let arguments = segment
                .arguments
                .iter()
                .map(generic_argument_string)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(match arguments.is_empty() {
                true => segment.name.as_str().to_owned(),
                false => format!("{}<{}>", segment.name.as_str(), arguments.join(", ")),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?
        .join("::");
    Ok(format!("{prefix}{segments}"))
}

fn generic_argument_string(argument: &GenericArgument) -> Result<String, Error> {
    match argument {
        GenericArgument::Type(type_expr) => type_string(type_expr),
        GenericArgument::Const(value) => const_expr_string(value),
        GenericArgument::AssociatedType { name, type_expr } => {
            Ok(format!("{} = {}", name.as_str(), type_string(type_expr)?))
        }
    }
}

fn type_string(type_expr: &TypeExpr) -> Result<String, Error> {
    rust_type(type_expr).map(|ty| quote!(#ty).to_string())
}

fn const_expr_string(value: &ConstExpr) -> Result<String, Error> {
    Ok(match value {
        ConstExpr::Raw(source) => source.clone(),
        ConstExpr::Path(path) => path_string(path)?,
        ConstExpr::Literal(literal) => literal_string(literal),
        ConstExpr::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(const_expr_string)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        ConstExpr::Tuple(values) => format!(
            "({})",
            values
                .iter()
                .map(const_expr_string)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
    })
}

fn literal_string(literal: &Literal) -> String {
    match literal {
        Literal::Bool(value) => value.to_string(),
        Literal::Integer(value) => value.source.clone(),
        Literal::Float(value) => value.source.clone(),
        Literal::String(value) => format!("{value:?}"),
        Literal::Bytes(value) => format!("{value:?}"),
    }
}

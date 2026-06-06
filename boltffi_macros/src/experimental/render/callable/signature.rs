use boltffi_ast::{
    ClosureType, FunctionDef, HandlePresence as SourceHandlePresence, ParameterDef, ReturnDef,
    TypeExpr,
};
use boltffi_binding::{HandlePresence, HandleTarget, Primitive};

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

    pub fn closure(self, presence: HandlePresence) -> Result<&'a ClosureType, Error> {
        let TypeExpr::Closure {
            signature,
            presence: source_presence,
        } = self.definition.rust_type.expr()
        else {
            return Err(Error::SourceSyntaxMismatch(
                "source parameter is not an inline closure",
            ));
        };
        Presence::new(*source_presence).matches(presence)?;
        Ok(signature.as_ref())
    }

    pub fn handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        Handle::new(self.definition.rust_type.expr()).matches(target, presence)
    }

    pub fn scalar_option(self, primitive: Primitive) -> Result<(), Error> {
        let TypeExpr::Option(inner) = self.definition.rust_type.expr() else {
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
        match self.definition.rust_type.expr() {
            TypeExpr::Vec(_) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter is not a direct vector",
            )),
        }
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

    pub fn closure(self, presence: HandlePresence) -> Result<&'a ClosureType, Error> {
        let ReturnDef::Value(rust_type) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an inline closure",
            ));
        };
        let TypeExpr::Closure {
            signature,
            presence: source_presence,
        } = rust_type.expr()
        else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an inline closure",
            ));
        };
        Presence::new(*source_presence).matches(presence)?;
        Ok(signature.as_ref())
    }

    pub fn handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        let ReturnDef::Value(value) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not a handle value",
            ));
        };
        Handle::new(value.expr()).matches(target, presence)
    }

    pub fn scalar_option(self, primitive: Primitive) -> Result<(), Error> {
        let ReturnDef::Value(rust_type) = self.definition else {
            return Err(Error::SourceSyntaxMismatch(
                "source return is not an optional scalar",
            ));
        };
        let TypeExpr::Option(inner) = rust_type.expr() else {
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
            ReturnDef::Value(rust_type) if matches!(rust_type.expr(), TypeExpr::Vec(_)) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source return is not a direct vector",
            )),
        }
    }

    pub fn fallible(self) -> Result<Fallible<'a>, Error> {
        let ReturnDef::Value(rust_type) = self.definition else {
            return Err(Error::SourceSyntaxMismatch("source return is not a Result"));
        };
        let TypeExpr::Result { ok, err } = rust_type.expr() else {
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

    pub fn ok_closure(self, presence: HandlePresence) -> Result<&'a ClosureType, Error> {
        let TypeExpr::Closure {
            signature,
            presence: source_presence,
        } = self.ok
        else {
            return Err(Error::SourceSyntaxMismatch(
                "source Result success is not an inline closure",
            ));
        };
        Presence::new(*source_presence).matches(presence)?;
        Ok(signature.as_ref())
    }

    pub fn ok_handle(self, target: &HandleTarget, presence: HandlePresence) -> Result<(), Error> {
        Handle::new(self.ok).matches(target, presence)
    }
}

struct Presence {
    source: SourceHandlePresence,
}

impl Presence {
    const fn new(source: SourceHandlePresence) -> Self {
        Self { source }
    }

    fn matches(self, binding: HandlePresence) -> Result<(), Error> {
        match (self.source, binding) {
            (SourceHandlePresence::Required, HandlePresence::Required)
            | (SourceHandlePresence::Nullable, HandlePresence::Nullable) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "source closure presence does not match binding closure presence",
            )),
        }
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
        match (self.source, target) {
            (
                TypeExpr::Class {
                    presence: source_presence,
                    ..
                },
                HandleTarget::Class(_),
            )
            | (
                TypeExpr::Trait {
                    presence: source_presence,
                    ..
                },
                HandleTarget::Callback(_),
            ) => Presence::new(*source_presence).matches(presence),
            _ => Err(Error::SourceSyntaxMismatch(
                "source handle type does not match binding handle target",
            )),
        }
    }
}

//! C parameters for generated closure trampolines.
//!
//! Rust calls inline closures through C function pointers. This module records
//! the exact C parameters accepted by the generated trampoline before they are
//! projected into JVM arguments.

use crate::{
    bridge::c::{self, Identifier, Statement, TypeFragment},
    core::Result,
};

/// One C parameter used by a generated closure bridge function.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureCParameter {
    name: Identifier,
    ty: TypeFragment,
    declaration: Statement,
}

impl ClosureCParameter {
    /// Creates a C parameter from explicit syntax fragments.
    pub fn new(name: Identifier, ty: TypeFragment) -> Self {
        Self {
            declaration: Statement::new(format!("{ty} {name}")),
            name,
            ty,
        }
    }

    /// Returns the C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C parameter type.
    pub fn ty(&self) -> &TypeFragment {
        &self.ty
    }

    /// Returns this parameter as a C declaration.
    pub fn declaration(&self) -> &Statement {
        &self.declaration
    }

    pub(in crate::bridge::jni::contract::closure) fn from_parameter(
        parameter: &c::Parameter,
    ) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(parameter.name())?,
            ty: TypeFragment::anonymous(parameter.ty())?,
            declaration: TypeFragment::declaration(parameter.ty(), parameter.name())?,
        })
    }
}

//! C callback-slot parameters.
//!
//! Generated callback functions are still C vtable slots. This module keeps the
//! C parameter name and type for those slots so callback templates can declare
//! the exact ABI Rust will call.

use crate::{
    bridge::c::{self, Identifier, Statement, TypeFragment},
    core::Result,
};

/// One C ABI parameter accepted by a generated callback vtable slot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCParameter {
    name: Identifier,
    ty: TypeFragment,
    declaration: Statement,
}

impl CallbackCParameter {
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

    pub(in crate::bridge::jni::contract::callback) fn from_parameter(
        parameter: &c::Parameter,
    ) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(parameter.name())?,
            ty: TypeFragment::anonymous(parameter.ty())?,
            declaration: TypeFragment::declaration(parameter.ty(), parameter.name())?,
        })
    }
}

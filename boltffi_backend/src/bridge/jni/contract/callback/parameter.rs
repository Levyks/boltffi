//! Callback-handle parameters for JNI native methods.
//!
//! Foreign JVM callback implementations enter Rust as opaque handle tokens.
//! This module maps that token through the C bridge callback constructor so Rust
//! receives the callback handle shape selected by lowering.

use crate::{
    bridge::c::{self, ArgumentList, Expression, Identifier, TypeFragment},
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI callback handle parameter mapped through a C callback constructor.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackParameter {
    name: Identifier,
    create_handle: Identifier,
}

impl CallbackParameter {
    /// Returns the generated JNI callback-handle parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        TypeFragment::new("jlong")
    }

    /// Returns the expression passed to the C bridge function.
    pub fn c_argument(&self) -> Expression {
        Expression::call(
            self.create_handle.clone(),
            ArgumentList::from_iter([Expression::cast(
                TypeFragment::new("uint64_t"),
                Expression::identifier(self.name.clone()),
            )]),
        )
    }

    /// Creates a callback parameter from one C callback-handle ABI parameter.
    pub fn from_c_parameter(
        parameter: &c::Parameter,
        callbacks: &[c::Callback],
    ) -> Result<Option<Self>> {
        let c::Type::CallbackHandle(callback) = parameter.ty() else {
            return Ok(None);
        };
        let declaration = callbacks
            .iter()
            .find(|declaration| declaration.id() == *callback)
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback handle parameter has no C callback declaration",
            })?;
        Ok(Some(Self {
            name: Identifier::escape(parameter.name())?,
            create_handle: Identifier::parse(declaration.create_handle().name())?,
        }))
    }
}

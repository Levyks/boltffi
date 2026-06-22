//! Closure parameters for JNI native methods.
//!
//! Foreign JVM closures enter Rust through a handle token. This module maps that
//! token to the call, context, and release C arguments expected by the lower C
//! bridge.

use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::ClosureRegistration,
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI parameter carrying a foreign closure handle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureParameter {
    name: Identifier,
    call: Identifier,
    release: Identifier,
}

impl ClosureParameter {
    /// Returns the generated JNI parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        TypeFragment::new("jlong")
    }

    /// Returns the C bridge arguments for this closure parameter.
    pub fn c_arguments(&self) -> Vec<Expression> {
        vec![
            Expression::identifier(self.call.clone()),
            Expression::cast(
                TypeFragment::new("void *"),
                Expression::identifier(self.name.clone()),
            ),
            Expression::identifier(self.release.clone()),
        ]
    }

    /// Creates a JNI closure parameter from one C closure parameter group.
    pub fn from_c_group(
        group: &c::ClosureParameter,
        registrations: &[ClosureRegistration],
    ) -> Result<Self> {
        let registration = registrations
            .iter()
            .find(|registration| registration.signature() == group.signature())
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure parameter has no JNI closure registration",
            })?;
        Ok(Self {
            name: Identifier::escape(group.name())?,
            call: registration.call().clone(),
            release: registration.release().clone(),
        })
    }
}

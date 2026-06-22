//! Closure returns from JVM callback methods.
//!
//! A callback method can return an inline closure through a C out-pointer. This
//! module keeps the out-pointer parameter, JVM handle token, and release path
//! tied to that returned closure.

use crate::{
    bridge::{
        c::{self, Identifier, Statement, TypeFragment},
        jni::{CallbackCParameter, ClosureRegistration},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// Closure returned from a JVM callback method through a C out-pointer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackClosureReturn {
    output: CallbackCParameter,
    storage: Identifier,
    invoke_field: Statement,
    invoke: Identifier,
    release: Identifier,
}

impl CallbackClosureReturn {
    /// Returns the C out-pointer parameter.
    pub fn output(&self) -> &CallbackCParameter {
        &self.output
    }

    /// Returns the C storage type written through the out-pointer.
    pub fn storage(&self) -> &Identifier {
        &self.storage
    }

    /// Returns the closure invoke field declaration.
    pub fn invoke_field(&self) -> &Statement {
        &self.invoke_field
    }

    /// Returns the C closure invoke function.
    pub fn invoke(&self) -> &Identifier {
        &self.invoke
    }

    /// Returns the C closure release function.
    pub fn release(&self) -> &Identifier {
        &self.release
    }

    /// Builds a closure return contract from a C callback method return slot.
    pub fn from_return(
        slot: &c::CallbackSlot,
        returned: &c::ClosureReturnParameter,
        registrations: &[ClosureRegistration],
    ) -> Result<Self> {
        let registration = registrations
            .iter()
            .find(|registration| registration.signature() == returned.signature())
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback closure return has no JNI closure registration",
            })?;
        Ok(Self {
            output: CallbackCParameter::from_parameter(slot.parameter(returned.output()))?,
            storage: Identifier::parse(format!(
                "BoltFFIJniClosureReturn{}",
                returned.signature().as_str()
            ))?,
            invoke_field: TypeFragment::declaration(returned.call_type(), "invoke")?,
            invoke: registration.call().clone(),
            release: registration.release().clone(),
        })
    }
}

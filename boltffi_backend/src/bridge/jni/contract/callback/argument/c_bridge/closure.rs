//! Closure callback arguments from C callback slots.
//!
//! Rust can pass inline closures into a JVM callback method. The C bridge carries
//! those closures as function pointer, context, and release parameters; this
//! module groups them into one JVM closure handle.

use crate::{
    bridge::{
        c::{self, Identifier},
        jni::{CallbackCParameter, ClosureRegistration},
    },
    core::{Error, Result},
};

use super::super::{CallbackArgument, CallbackArgumentKind};

const JNI_BRIDGE: &str = "jni";

pub fn from_group(
    slot: &c::CallbackSlot,
    closure: &c::ClosureParameter,
    registrations: &[ClosureRegistration],
) -> Result<CallbackArgument> {
    let registration = registrations
        .iter()
        .find(|registration| registration.signature() == closure.signature())
        .ok_or(Error::BrokenBridgeContract {
            bridge: JNI_BRIDGE,
            invariant: "callback closure parameter has no JNI closure registration",
        })?;
    let handle = registration
        .callback_handle()
        .ok_or(Error::BrokenBridgeContract {
            bridge: JNI_BRIDGE,
            invariant: "callback closure parameter has no JNI closure handle",
        })?;

    Ok(CallbackArgument {
        kind: CallbackArgumentKind::Closure {
            handle: Identifier::parse(format!("__boltffi_{}_handle", closure.name()))?,
            call: CallbackCParameter::from_parameter(slot.parameter(closure.call()))?,
            context: CallbackCParameter::from_parameter(slot.parameter(closure.context()))?,
            release: CallbackCParameter::from_parameter(slot.parameter(closure.release()))?,
            handle_new: handle.new_function().clone(),
            handle_release: handle.release_function().clone(),
        },
    })
}

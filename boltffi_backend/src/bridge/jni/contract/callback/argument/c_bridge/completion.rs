//! Async-completion callback arguments from C callback slots.
//!
//! Async callback methods receive a completion function pointer and context from
//! Rust. This module keeps that pair as one JVM argument group so the Java side
//! can complete or fail the callback later.

use crate::{
    bridge::{
        c,
        jni::{CallbackCParameter, CallbackCompletionPayload},
    },
    core::{Error, Result},
};

use super::super::{CallbackArgument, CallbackArgumentKind};

const JNI_BRIDGE: &str = "jni";

pub fn from_group(
    slot: &c::CallbackSlot,
    completion: &c::CallbackCompletionParameter,
    callbacks: &[c::Callback],
) -> Result<CallbackArgument> {
    let callback = slot.parameter(completion.callback());
    let payload = match callback.ty() {
        c::Type::FunctionPointer { params, .. } => match params.as_slice() {
            [c::Type::MutPointer(context), c::Type::Status]
                if matches!(context.as_ref(), c::Type::Void) =>
            {
                None
            }
            [c::Type::MutPointer(context), c::Type::Status, payload]
                if matches!(context.as_ref(), c::Type::Void) =>
            {
                Some(CallbackCompletionPayload::from_c_type(payload, callbacks)?)
            }
            _ => {
                return Err(Error::BrokenBridgeContract {
                    bridge: JNI_BRIDGE,
                    invariant: "callback completion function pointer has unexpected parameters",
                });
            }
        },
        _ => {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback completion parameter is not a function pointer",
            });
        }
    };
    Ok(CallbackArgument {
        kind: CallbackArgumentKind::Completion {
            callback: CallbackCParameter::from_parameter(callback)?,
            context: CallbackCParameter::from_parameter(slot.parameter(completion.context()))?,
            payload,
        },
    })
}

//! Borrowed-byte callback arguments from C callback slots.
//!
//! The C bridge passes encoded callback payloads as pointer and length
//! parameters. This module groups those raw parameters into one JVM byte-array
//! argument.

use crate::{
    bridge::{
        c::{self, Identifier},
        jni::CallbackCParameter,
    },
    core::Result,
};

use super::super::{CallbackArgument, CallbackArgumentKind};

pub fn from_group(
    slot: &c::CallbackSlot,
    bytes: &c::ByteSliceParameter,
) -> Result<CallbackArgument> {
    Ok(CallbackArgument {
        kind: CallbackArgumentKind::Bytes {
            name: Identifier::escape(bytes.name())?,
            pointer: CallbackCParameter::from_parameter(slot.parameter(bytes.pointer()))?,
            length: CallbackCParameter::from_parameter(slot.parameter(bytes.length()))?,
        },
    })
}

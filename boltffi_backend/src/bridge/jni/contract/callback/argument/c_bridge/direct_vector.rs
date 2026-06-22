//! Direct-vector callback arguments from C callback slots.
//!
//! Direct vectors passed from Rust to a JVM callback arrive as pointer and length
//! C parameters. This module turns that pair into one Java primitive array
//! argument with the right JNI element type.

use crate::{
    bridge::{
        c::{self, Identifier},
        jni::{CallbackCParameter, JniType},
    },
    core::Result,
};

use super::super::{CallbackArgument, CallbackArgumentKind};

pub fn from_group(
    slot: &c::CallbackSlot,
    vector: &c::DirectVectorParameter,
) -> Result<CallbackArgument> {
    Ok(CallbackArgument {
        kind: CallbackArgumentKind::DirectVector {
            array: Identifier::escape(vector.name())?,
            pointer: CallbackCParameter::from_parameter(slot.parameter(vector.pointer()))?,
            length: CallbackCParameter::from_parameter(slot.parameter(vector.length()))?,
            jni_type: JniType::from_direct_vector_element(vector.element())?,
        },
    })
}

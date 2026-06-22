//! Template view for closure byte-array arguments.
//!
//! Closure call templates need a Java byte array local plus the C pointer and
//! length inputs that fill it. This module prepares those fields.

use crate::bridge::{c::Identifier, jni::ClosureBytesArgument};

pub struct ClosureBytesArgumentView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
    pub buffer: Identifier,
}

impl ClosureBytesArgumentView {
    pub fn from_argument(argument: &ClosureBytesArgument) -> Self {
        Self {
            name: argument.name().clone(),
            pointer: argument.pointer().clone(),
            length: argument.length().clone(),
            buffer: argument.buffer().clone(),
        }
    }
}

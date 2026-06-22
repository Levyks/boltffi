//! Template view for nested closure handles.
//!
//! Nested closures are passed to the JVM as handle tokens. This module prepares
//! the symbols needed to allocate, call, and release those handles.

use crate::bridge::{c::Identifier, jni::ClosureHandleArgument};

pub struct ClosureHandleArgumentView {
    pub handle: Identifier,
    pub call: Identifier,
    pub context: Identifier,
    pub release: Identifier,
    pub handle_new: Identifier,
    pub handle_release: Identifier,
}

impl ClosureHandleArgumentView {
    pub fn from_argument(argument: &ClosureHandleArgument) -> Self {
        Self {
            handle: argument.handle().clone(),
            call: argument.call().clone(),
            context: argument.context().clone(),
            release: argument.release().clone(),
            handle_new: argument.handle_new().clone(),
            handle_release: argument.handle_release().clone(),
        }
    }
}

//! Direct-record arguments passed into JVM callbacks.
//!
//! Direct records sent from Rust into a JVM callback are copied into Java byte
//! arrays. This module ties the array local to the original C record parameter.

use crate::bridge::c::Identifier;

/// Direct-record argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackRecordArgument<'argument> {
    array: &'argument Identifier,
    parameter: &'argument Identifier,
}

impl<'argument> CallbackRecordArgument<'argument> {
    pub(in crate::bridge::jni::contract::callback) fn new(
        array: &'argument Identifier,
        parameter: &'argument Identifier,
    ) -> Self {
        Self { array, parameter }
    }

    /// Returns the local JNI byte-array variable.
    pub fn array(&self) -> &Identifier {
        self.array
    }

    /// Returns the C record parameter.
    pub fn parameter(&self) -> &Identifier {
        self.parameter
    }
}

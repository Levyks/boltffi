//! Callback handles passed into JVM callbacks.
//!
//! Callback methods can receive another callback handle from Rust. This module
//! keeps the C callback handle parameter and the JVM token built from it
//! together.

use crate::bridge::c::Identifier;

/// Callback-handle argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackHandleArgument<'argument> {
    handle: &'argument Identifier,
    parameter: &'argument Identifier,
}

impl<'argument> CallbackHandleArgument<'argument> {
    pub(in crate::bridge::jni::contract::callback) fn new(
        handle: &'argument Identifier,
        parameter: &'argument Identifier,
    ) -> Self {
        Self { handle, parameter }
    }

    /// Returns the local JVM callback-handle token.
    pub fn handle(&self) -> &Identifier {
        self.handle
    }

    /// Returns the C callback-handle parameter.
    pub fn parameter(&self) -> &Identifier {
        self.parameter
    }
}

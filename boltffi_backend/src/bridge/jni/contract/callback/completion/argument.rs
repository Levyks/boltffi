//! Callback completion failure arguments.
//!
//! Async callback failures are reported through a generated native completion
//! method. This module models the status and error-buffer arguments that the JVM
//! passes back into Rust.

use crate::bridge::{
    c::{ArgumentList, Identifier},
    jni::CallbackCompletionPayload,
};

/// Completion callback invoked when async JVM callback dispatch fails.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionArgument<'argument> {
    callback: &'argument Identifier,
    failure_arguments: ArgumentList,
    payload: Option<CallbackCompletionPayload>,
}

impl<'argument> CallbackCompletionArgument<'argument> {
    pub(in crate::bridge::jni::contract::callback) fn new(
        callback: &'argument Identifier,
        failure_arguments: ArgumentList,
        payload: Option<CallbackCompletionPayload>,
    ) -> Self {
        Self {
            callback,
            failure_arguments,
            payload,
        }
    }

    /// Returns the C completion callback parameter.
    pub fn callback(&self) -> &Identifier {
        self.callback
    }

    /// Returns arguments that complete the async callback with failure.
    pub fn failure_arguments(&self) -> &ArgumentList {
        &self.failure_arguments
    }

    /// Returns the payload carried by successful callback completion.
    pub fn payload(&self) -> Option<&CallbackCompletionPayload> {
        self.payload.as_ref()
    }
}

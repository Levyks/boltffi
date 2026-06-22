use crate::bridge::jni::CallbackCParameter;

use super::{CallbackArgument, CallbackArgumentKind};

impl CallbackArgument {
    /// Returns the C ABI parameters that carry this callback argument.
    pub fn c_parameters(&self) -> Vec<CallbackCParameter> {
        match &self.kind {
            CallbackArgumentKind::Value { parameter, .. } => vec![parameter.clone()],
            CallbackArgumentKind::Bytes {
                pointer, length, ..
            }
            | CallbackArgumentKind::DirectVector {
                pointer, length, ..
            } => vec![pointer.clone(), length.clone()],
            CallbackArgumentKind::Record { parameter, .. }
            | CallbackArgumentKind::CallbackHandle { parameter, .. } => vec![parameter.clone()],
            CallbackArgumentKind::Closure {
                call,
                context,
                release,
                ..
            } => vec![call.clone(), context.clone(), release.clone()],
            CallbackArgumentKind::Completion {
                callback, context, ..
            } => vec![callback.clone(), context.clone()],
        }
    }
}

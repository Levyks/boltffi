use crate::bridge::{
    c::{ArgumentList, Expression, Literal, TypeFragment},
    jni::{
        CallbackBytesArgument, CallbackClosureArgument, CallbackCompletionArgument,
        CallbackDirectVectorArgument, CallbackHandleArgument, CallbackRecordArgument,
    },
};

use super::{CallbackArgument, CallbackArgumentKind};

impl CallbackArgument {
    /// Returns byte-array setup data when this argument carries borrowed bytes.
    pub fn bytes(&self) -> Option<CallbackBytesArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. } | CallbackArgumentKind::DirectVector { .. } => None,
            CallbackArgumentKind::Bytes {
                name,
                pointer,
                length,
            } => Some(CallbackBytesArgument::new(
                name,
                pointer.name(),
                length.name(),
            )),
            CallbackArgumentKind::Record { .. } | CallbackArgumentKind::CallbackHandle { .. } => {
                None
            }
            CallbackArgumentKind::Closure { .. } => None,
            CallbackArgumentKind::Completion { .. } => None,
        }
    }

    /// Returns direct-vector setup data when this argument carries a Java array.
    pub fn direct_vector(&self) -> Option<CallbackDirectVectorArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. }
            | CallbackArgumentKind::Closure { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::DirectVector {
                array,
                pointer,
                length,
                jni_type,
            } => Some(CallbackDirectVectorArgument::new(
                array,
                pointer.name(),
                length.name(),
                *jni_type,
            )),
        }
    }

    /// Returns record-array setup data when this argument carries a direct record.
    pub fn record(&self) -> Option<CallbackRecordArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. } => None,
            CallbackArgumentKind::CallbackHandle { .. } => None,
            CallbackArgumentKind::Closure { .. } => None,
            CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::Record { array, parameter } => {
                Some(CallbackRecordArgument::new(array, parameter.name()))
            }
        }
    }

    /// Returns callback-handle setup data when this argument carries a callback handle.
    pub fn callback_handle(&self) -> Option<CallbackHandleArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::Closure { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::CallbackHandle { handle, parameter } => {
                Some(CallbackHandleArgument::new(handle, parameter.name()))
            }
        }
    }

    /// Returns closure-handle setup data when this argument carries a Rust-owned closure.
    pub fn closure_handle(&self) -> Option<CallbackClosureArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::Closure {
                handle,
                call,
                context,
                release,
                handle_new,
                handle_release,
            } => Some(CallbackClosureArgument::new(
                handle,
                call.name(),
                context.name(),
                release.name(),
                handle_new,
                handle_release,
            )),
        }
    }

    /// Returns completion callback details for async callback methods.
    pub fn completion(&self) -> Option<CallbackCompletionArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. } => None,
            CallbackArgumentKind::Closure { .. } => None,
            CallbackArgumentKind::Completion {
                callback,
                context,
                payload,
            } => Some(CallbackCompletionArgument::new(
                callback.name(),
                ArgumentList::from_iter(
                    [
                        Expression::identifier(context.name().clone()),
                        Expression::cast(
                            TypeFragment::new("FfiStatus"),
                            Expression::literal(Literal::status_failure()),
                        ),
                    ]
                    .into_iter()
                    .chain(payload.iter().map(|payload| {
                        Expression::cast(
                            payload.clone(),
                            Expression::literal(Literal::compound_zero()),
                        )
                    })),
                ),
            )),
        }
    }
}

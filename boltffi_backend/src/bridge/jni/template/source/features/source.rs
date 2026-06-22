use crate::bridge::jni::template::{
    callback::{CallbackCompletionInvokerView, CallbackRegistrationView},
    closure::{CallbackClosureHandleView, ClosureRegistrationView},
    method::NativeMethodView,
    stream::DirectStreamBatchView,
};

use super::{
    callback::CallbackFeatures, closure::ClosureFeatures, completion::CompletionFeatures,
    method::MethodFeatures, stream::StreamFeatures,
};

pub struct SourceFeatures {
    pub uses_limits: bool,
    pub checks_status: bool,
    pub uses_byte_arrays: bool,
    pub uses_record_arrays: bool,
    pub uses_exceptions: bool,
    pub uses_lifecycle: bool,
    pub uses_continuations: bool,
    pub uses_callback_handles: bool,
    pub uses_closure_handles: bool,
}

impl SourceFeatures {
    pub fn from_views(
        methods: &[NativeMethodView],
        direct_stream_batches: &[DirectStreamBatchView],
        callbacks: &[CallbackRegistrationView],
        callback_completions: &[CallbackCompletionInvokerView],
        closures: &[ClosureRegistrationView],
        closure_handles: &[CallbackClosureHandleView],
    ) -> Self {
        let methods = MethodFeatures::from_methods(methods);
        let callbacks = CallbackFeatures::from_registrations(callbacks);
        let completions = CompletionFeatures::from_invokers(callback_completions);
        let closures = ClosureFeatures::from_registrations(closures);
        let streams = StreamFeatures::from_direct_batches(direct_stream_batches);
        let uses_closure_handles = !closure_handles.is_empty();
        let uses_byte_arrays = callbacks.uses_byte_arrays
            || closures.uses_byte_arrays
            || callbacks.returns_byte_arrays
            || closures.returns_byte_arrays
            || methods.returns_byte_arrays
            || completions.uses_byte_arrays
            || streams.returns_direct_batches;
        let uses_record_arrays = methods.uses_record_arrays
            || callbacks.uses_record_arrays
            || closures.returns_records
            || callbacks.returns_records
            || completions.uses_record_arrays;

        Self {
            uses_limits: uses_byte_arrays
                || uses_record_arrays
                || callbacks.uses_direct_vectors
                || closures.uses_direct_vectors,
            checks_status: methods.checks_status,
            uses_byte_arrays,
            uses_record_arrays,
            uses_exceptions: callbacks.uses_byte_arrays
                || callbacks.uses_direct_vectors
                || callbacks.uses_record_arrays
                || callbacks.uses_handles
                || uses_closure_handles
                || closures.uses_byte_arrays
                || closures.uses_direct_vectors
                || callbacks.returns_byte_arrays
                || closures.returns_byte_arrays
                || callbacks.returns_callback_handles
                || closures.returns_callback_handles
                || completions.uses_byte_arrays
                || streams.returns_direct_batches
                || methods.uses_exceptions,
            uses_continuations: methods.uses_continuations,
            uses_lifecycle: methods.uses_continuations
                || callbacks.has_registrations
                || closures.has_registrations,
            uses_callback_handles: callbacks.uses_handles
                || callbacks.returns_callback_handles
                || closures.returns_callback_handles
                || methods.returns_callback_handles,
            uses_closure_handles,
        }
    }
}

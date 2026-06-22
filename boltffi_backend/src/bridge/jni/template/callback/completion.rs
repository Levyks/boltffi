//! Template views for async callback completion invokers.
//!
//! Completion invokers are native methods called later by the JVM. This module
//! prepares the success and failure method views, including payload-specific JNI
//! types and C bridge call arguments.

use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::{CallbackCompletionInvoker, CallbackCompletionPayload},
};

pub struct CallbackCompletionInvokerView {
    pub success: Identifier,
    pub failure: Identifier,
    pub has_payload: bool,
    pub payload_c_type: TypeFragment,
    pub payload_jni_type: TypeFragment,
    pub payload_bytes: bool,
    pub payload_record: bool,
    pub payload_callback_handle: bool,
    pub payload_create_handle: Option<Identifier>,
}

impl CallbackCompletionInvokerView {
    pub fn from_invoker(invoker: &CallbackCompletionInvoker) -> Self {
        let payload = invoker.payload();
        Self {
            success: invoker.success().as_identifier().clone(),
            failure: invoker.failure().as_identifier().clone(),
            has_payload: payload.is_some(),
            payload_c_type: payload
                .map(CallbackCompletionPayload::c_type)
                .cloned()
                .unwrap_or_else(|| TypeFragment::new("void")),
            payload_jni_type: payload
                .map(CallbackCompletionPayload::jni_type)
                .cloned()
                .unwrap_or_else(|| TypeFragment::new("void")),
            payload_bytes: payload.is_some_and(CallbackCompletionPayload::is_bytes),
            payload_record: payload.is_some_and(CallbackCompletionPayload::is_record),
            payload_callback_handle: payload
                .and_then(CallbackCompletionPayload::callback_handle_constructor)
                .is_some(),
            payload_create_handle: payload
                .and_then(CallbackCompletionPayload::callback_handle_constructor)
                .cloned(),
        }
    }
}

//! Template view for callback vtable methods.
//!
//! Callback method templates render the C vtable slot that calls a static JVM
//! method. This module prepares method ids, parameter declarations, local setup,
//! return handling, and cleanup views.

use crate::bridge::{
    c::{ArgumentList, Expression, Identifier, Literal, TypeFragment},
    jni::CallbackMethod,
};

use super::{
    CallbackBytesArgumentView, CallbackCParameterView, CallbackClosureArgumentView,
    CallbackClosureReturnView, CallbackCompletionArgumentView, CallbackDirectVectorArgumentView,
    CallbackHandleArgumentView, CallbackRecordArgumentView,
};

pub struct CallbackMethodView {
    pub function: Identifier,
    pub method: Identifier,
    pub method_id: Identifier,
    pub signature: Literal,
    pub c_return_type: TypeFragment,
    pub returns_void: bool,
    pub returns_byte_array: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub returns_callback_handle: bool,
    pub returns_closure: bool,
    pub callback_handle_constructor: Option<Identifier>,
    pub closure_return: Option<CallbackClosureReturnView>,
    pub call_method_suffix: String,
    pub failure_value: Expression,
    pub c_parameters: Vec<CallbackCParameterView>,
    pub byte_arrays: Vec<CallbackBytesArgumentView>,
    pub direct_vectors: Vec<CallbackDirectVectorArgumentView>,
    pub record_arrays: Vec<CallbackRecordArgumentView>,
    pub callback_handles: Vec<CallbackHandleArgumentView>,
    pub closure_handles: Vec<CallbackClosureArgumentView>,
    pub completions: Vec<CallbackCompletionArgumentView>,
    pub jni_arguments: ArgumentList,
}

impl CallbackMethodView {
    pub fn from_method(method: &CallbackMethod) -> Self {
        Self {
            function: method.function().clone(),
            method: method.method().clone(),
            method_id: method.method_id().clone(),
            signature: Literal::string(method.signature()),
            c_return_type: method.c_return_type().clone(),
            returns_void: method.returns_void(),
            returns_byte_array: method.returns_byte_array(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            returns_callback_handle: method.returns_callback_handle(),
            returns_closure: method.returns_closure(),
            callback_handle_constructor: method.callback_handle_constructor().cloned(),
            closure_return: method
                .closure_return()
                .map(CallbackClosureReturnView::from_return),
            call_method_suffix: method.call_method_suffix().unwrap_or_default().to_owned(),
            failure_value: method
                .failure_value()
                .unwrap_or_else(|| Expression::literal(Literal::integer_zero())),
            c_parameters: method
                .c_parameters()
                .iter()
                .map(CallbackCParameterView::from_parameter)
                .collect(),
            byte_arrays: method
                .byte_arrays()
                .iter()
                .map(CallbackBytesArgumentView::from_argument)
                .collect(),
            direct_vectors: method
                .direct_vectors()
                .iter()
                .map(CallbackDirectVectorArgumentView::from_argument)
                .collect(),
            record_arrays: method
                .record_arrays()
                .iter()
                .map(CallbackRecordArgumentView::from_argument)
                .collect(),
            callback_handles: method
                .callback_handles()
                .iter()
                .map(CallbackHandleArgumentView::from_argument)
                .collect(),
            closure_handles: method
                .closure_handles()
                .iter()
                .map(CallbackClosureArgumentView::from_argument)
                .collect(),
            completions: method
                .completions()
                .iter()
                .map(CallbackCompletionArgumentView::from_argument)
                .collect(),
            jni_arguments: method.jni_arguments(),
        }
    }
}

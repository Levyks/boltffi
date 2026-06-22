//! Root JNI source-file template input.
//!
//! This module builds the full Askama context for the generated `jni_glue.c`
//! file: header include, lifecycle symbols, native methods, callbacks,
//! closures, streams, and the feature flags that select runtime fragments.

use askama::Template as AskamaTemplate;

use super::features::SourceFeatures;
use crate::{
    bridge::{
        c::{Identifier, Literal},
        jni::{
            JniBridgeContract, JniSymbolName,
            template::{
                callback::{CallbackCompletionInvokerView, CallbackRegistrationView},
                closure::{CallbackClosureHandleView, ClosureRegistrationView},
                method::NativeMethodView,
                stream::DirectStreamBatchView,
            },
        },
    },
    core::Result,
};

#[derive(AskamaTemplate)]
#[template(path = "bridge/jni/source.c", escape = "none")]
struct SourceFileTemplate {
    c_header: Literal,
    class_name: Literal,
    free_buffer: Identifier,
    uses_limits: bool,
    checks_status: bool,
    uses_byte_arrays: bool,
    uses_record_arrays: bool,
    uses_exceptions: bool,
    uses_lifecycle: bool,
    uses_continuations: bool,
    uses_callback_handles: bool,
    uses_closure_handles: bool,
    callback_clone_symbol: Identifier,
    callback_release_symbol: Identifier,
    callbacks: Vec<CallbackRegistrationView>,
    callback_completions: Vec<CallbackCompletionInvokerView>,
    closure_handles: Vec<CallbackClosureHandleView>,
    closures: Vec<ClosureRegistrationView>,
    methods: Vec<NativeMethodView>,
    direct_stream_batches: Vec<DirectStreamBatchView>,
}

/// JNI C source rendered from a JNI bridge contract.
pub struct SourceFile;

impl SourceFile {
    /// Renders the generated JNI C source file.
    pub fn render(contract: &JniBridgeContract) -> Result<String> {
        let methods = contract
            .methods()
            .iter()
            .map(NativeMethodView::from_method)
            .collect::<Result<Vec<_>>>()?;
        let callbacks = contract
            .callbacks()
            .iter()
            .map(CallbackRegistrationView::from_registration)
            .collect::<Vec<_>>();
        let callback_completions = contract
            .callback_completions()
            .iter()
            .map(CallbackCompletionInvokerView::from_invoker)
            .collect::<Vec<_>>();
        let closures = contract
            .closures()
            .iter()
            .map(ClosureRegistrationView::from_registration)
            .collect::<Vec<_>>();
        let direct_stream_batches = contract
            .streams()
            .iter()
            .flat_map(|stream| stream.direct_batches())
            .map(DirectStreamBatchView::from_method)
            .collect::<Result<Vec<_>>>()?;
        let stream_methods = contract
            .streams()
            .iter()
            .flat_map(|stream| stream.methods())
            .map(NativeMethodView::from_method)
            .collect::<Result<Vec<_>>>()?;
        let methods = methods
            .into_iter()
            .chain(stream_methods)
            .collect::<Vec<_>>();
        let closure_handles = contract
            .closures()
            .iter()
            .filter_map(CallbackClosureHandleView::from_registration)
            .collect::<Vec<_>>();
        let features = SourceFeatures::from_views(
            &methods,
            &direct_stream_batches,
            &callbacks,
            &callback_completions,
            &closures,
            &closure_handles,
        );
        Ok(SourceFileTemplate {
            c_header: Literal::string(contract.c_header().as_str()),
            class_name: Literal::string(&contract.class().as_jni_class_name()),
            free_buffer: contract.free_buffer().clone(),
            uses_limits: features.uses_limits,
            checks_status: features.checks_status,
            uses_byte_arrays: features.uses_byte_arrays,
            uses_record_arrays: features.uses_record_arrays,
            uses_exceptions: features.uses_exceptions,
            uses_lifecycle: features.uses_lifecycle,
            uses_continuations: features.uses_continuations,
            uses_callback_handles: features.uses_callback_handles,
            uses_closure_handles: features.uses_closure_handles,
            callback_clone_symbol: JniSymbolName::native_method(
                contract.class(),
                "boltffi_callback_handle_clone",
            )?
            .as_identifier()
            .clone(),
            callback_release_symbol: JniSymbolName::native_method(
                contract.class(),
                "boltffi_callback_handle_release",
            )?
            .as_identifier()
            .clone(),
            callbacks,
            callback_completions,
            closure_handles,
            closures,
            methods,
            direct_stream_batches,
        }
        .render()?)
    }
}

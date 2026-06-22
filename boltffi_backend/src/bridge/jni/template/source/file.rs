//! Root JNI source-file template input.
//!
//! The JNI bridge emits one C source file. That file combines the lower C header
//! include, lifecycle hooks, `Java_*` native methods, callback vtables, closure
//! trampolines, stream helpers, and support fragments selected by the feature
//! scan.
//!
//! This module builds the Askama context for that root file from the finished
//! JNI bridge contract. It is the final assembly step before template rendering;
//! it does not decide ABI support or inspect the binding IR.

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
            .collect::<Result<Vec<_>>>()?;
        let callback_completions = contract
            .callback_completions()
            .iter()
            .map(CallbackCompletionInvokerView::from_invoker)
            .collect::<Vec<_>>();
        let closures = contract
            .closures()
            .iter()
            .map(ClosureRegistrationView::from_registration)
            .collect::<Result<Vec<_>>>()?;
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
            .map(CallbackClosureHandleView::from_registration)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let features = SourceFeatures::from_views(
            &methods,
            &direct_stream_batches,
            &callbacks,
            &callback_completions,
            &closures,
            &closure_handles,
        );
        let rendered = SourceFileTemplate {
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
        .render()?;

        Ok(Self::format_source(rendered))
    }

    fn format_source(rendered: String) -> String {
        let mut previous_blank = false;
        let mut source = rendered
            .lines()
            .fold(Vec::new(), |mut lines, line| {
                let blank = line.trim().is_empty();
                if blank {
                    if !previous_blank {
                        lines.push(line);
                    }
                    previous_blank = true;
                    return lines;
                }

                if lines
                    .last()
                    .is_some_and(|previous| Self::needs_section_break(previous, line))
                {
                    lines.push("");
                }

                previous_blank = blank;
                lines.push(line);
                lines
            })
            .join("\n");

        source.push('\n');
        source
    }

    fn needs_section_break(previous: &str, current: &str) -> bool {
        !previous.trim().is_empty()
            && !current.starts_with(' ')
            && !current.starts_with('\t')
            && matches!(previous, "}" | "};" | "#endif")
            && Self::starts_top_level_declaration(current)
    }

    fn starts_top_level_declaration(line: &str) -> bool {
        line.starts_with("JNIEXPORT")
            || line.starts_with("static ")
            || line.starts_with("typedef ")
            || line.starts_with("Ffi")
            || line.starts_with("Bolt")
    }
}

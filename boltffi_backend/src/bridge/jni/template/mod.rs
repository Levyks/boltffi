use askama::Template as AskamaTemplate;

mod callback;
mod closure;
mod method;

use self::callback::CallbackRegistrationView;
use self::closure::ClosureRegistrationView;
use self::method::NativeMethodView;

use crate::{
    bridge::{
        c::{Identifier, Literal},
        jni::{JniBridgeContract, JniSymbolName},
    },
    core::Result,
};

#[derive(AskamaTemplate)]
#[template(path = "bridge/jni/source.c", escape = "none")]
struct SourceFileTemplate {
    c_header: Literal,
    class_name: Literal,
    free_buffer: Identifier,
    checks_status: bool,
    uses_byte_arrays: bool,
    uses_record_arrays: bool,
    uses_exceptions: bool,
    uses_lifecycle: bool,
    uses_continuations: bool,
    uses_callback_handles: bool,
    callback_clone_symbol: Identifier,
    callback_release_symbol: Identifier,
    callbacks: Vec<CallbackRegistrationView>,
    closures: Vec<ClosureRegistrationView>,
    methods: Vec<NativeMethodView>,
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
        let callbacks: Vec<_> = contract
            .callbacks()
            .iter()
            .map(CallbackRegistrationView::from_registration)
            .collect();
        let closures: Vec<_> = contract
            .closures()
            .iter()
            .map(ClosureRegistrationView::from_registration)
            .collect();
        let callback_uses_byte_arrays = callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| !method.byte_arrays.is_empty())
        });
        let callback_uses_record_arrays = callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| !method.record_arrays.is_empty())
        });
        Ok(SourceFileTemplate {
            c_header: Literal::string(contract.c_header().as_str()),
            class_name: Literal::string(&contract.class().as_jni_class_name()),
            free_buffer: contract.free_buffer().clone(),
            checks_status: methods.iter().any(|method| method.checks_status),
            uses_byte_arrays: callback_uses_byte_arrays
                || callback_uses_record_arrays
                || methods.iter().any(|method| {
                    method.returns_bytes
                        || method.returns_record
                        || !method.byte_arrays.is_empty()
                        || !method.record_arrays.is_empty()
                }),
            uses_record_arrays: methods
                .iter()
                .any(|method| method.returns_record || !method.record_arrays.is_empty())
                || callback_uses_record_arrays,
            uses_exceptions: callback_uses_byte_arrays
                || callback_uses_record_arrays
                || methods.iter().any(|method| {
                    method.checks_status
                        || method.returns_bytes
                        || method.returns_record
                        || method.returns_callback
                        || !method.byte_arrays.is_empty()
                        || !method.record_arrays.is_empty()
                }),
            uses_continuations: methods.iter().any(|method| method.uses_continuations),
            uses_lifecycle: methods.iter().any(|method| method.uses_continuations)
                || !callbacks.is_empty()
                || !closures.is_empty(),
            uses_callback_handles: methods.iter().any(|method| method.returns_callback),
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
            closures,
            methods,
        }
        .render()?)
    }
}

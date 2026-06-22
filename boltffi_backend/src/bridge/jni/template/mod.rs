use askama::Template as AskamaTemplate;

mod callback;
mod closure;
mod features;
mod method;

use self::callback::CallbackRegistrationView;
use self::closure::{CallbackClosureHandleView, ClosureRegistrationView};
use self::features::SourceFeatures;
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
    uses_closure_handles: bool,
    callback_clone_symbol: Identifier,
    callback_release_symbol: Identifier,
    callbacks: Vec<CallbackRegistrationView>,
    closure_handles: Vec<CallbackClosureHandleView>,
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
        let closure_handles: Vec<_> = contract
            .closures()
            .iter()
            .filter_map(CallbackClosureHandleView::from_registration)
            .collect();
        let features =
            SourceFeatures::from_views(&methods, &callbacks, &closures, &closure_handles);
        Ok(SourceFileTemplate {
            c_header: Literal::string(contract.c_header().as_str()),
            class_name: Literal::string(&contract.class().as_jni_class_name()),
            free_buffer: contract.free_buffer().clone(),
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
            closure_handles,
            closures,
            methods,
        }
        .render()?)
    }
}

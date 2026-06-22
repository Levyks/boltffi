use askama::Template as AskamaTemplate;

mod callback;

use self::callback::CallbackRegistrationView;

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, Literal, TypeFragment},
        jni::{
            BytesParameter, JniBridgeContract, JniSymbolName, NativeMethod, NativeParameter,
            RecordParameter,
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
        Ok(SourceFileTemplate {
            c_header: Literal::string(contract.c_header().as_str()),
            class_name: Literal::string(&contract.class().as_jni_class_name()),
            free_buffer: contract.free_buffer().clone(),
            checks_status: methods.iter().any(|method| method.checks_status),
            uses_byte_arrays: methods.iter().any(|method| {
                method.returns_bytes
                    || method.returns_record
                    || !method.byte_arrays.is_empty()
                    || !method.record_arrays.is_empty()
            }),
            uses_record_arrays: methods
                .iter()
                .any(|method| method.returns_record || !method.record_arrays.is_empty()),
            uses_exceptions: methods.iter().any(|method| {
                method.checks_status
                    || method.returns_bytes
                    || method.returns_record
                    || method.returns_callback
                    || !method.byte_arrays.is_empty()
                    || !method.record_arrays.is_empty()
            }),
            uses_continuations: methods.iter().any(|method| method.uses_continuations),
            uses_lifecycle: methods.iter().any(|method| method.uses_continuations)
                || !callbacks.is_empty(),
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
            methods,
        }
        .render()?)
    }
}

struct NativeMethodView {
    symbol: Identifier,
    c_function: Identifier,
    return_type: TypeFragment,
    c_result_type: TypeFragment,
    parameters: Vec<NativeParameterView>,
    byte_arrays: Vec<ByteArrayParameterView>,
    record_arrays: Vec<RecordParameterView>,
    arguments: ArgumentList,
    returns_void: bool,
    returns_boolean: bool,
    returns_bytes: bool,
    returns_record: bool,
    returns_callback: bool,
    return_value: Expression,
    checks_status: bool,
    uses_continuations: bool,
}

impl NativeMethodView {
    fn from_method(method: &NativeMethod) -> Result<Self> {
        Ok(Self {
            symbol: method.symbol().as_identifier().clone(),
            c_function: Identifier::parse(method.c_function().name())?,
            return_type: method.returns().jni_type(),
            c_result_type: method.returns().c_result_type()?,
            parameters: method
                .parameters()
                .iter()
                .map(NativeParameterView::from_parameter)
                .collect(),
            byte_arrays: method
                .parameters()
                .iter()
                .filter_map(|parameter| parameter.bytes().map(ByteArrayParameterView::from_bytes))
                .collect(),
            record_arrays: method
                .parameters()
                .iter()
                .filter_map(|parameter| parameter.record().map(RecordParameterView::from_record))
                .collect(),
            arguments: ArgumentList::from_iter(
                method
                    .parameters()
                    .iter()
                    .map(NativeParameter::c_arguments)
                    .collect::<Result<Vec<_>>>()?
                    .into_iter()
                    .flatten(),
            ),
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            returns_bytes: method.returns_bytes(),
            returns_record: method.returns_record(),
            returns_callback: method.returns_callback(),
            return_value: method
                .returns()
                .return_expression(Expression::identifier(Identifier::parse("result")?))?,
            checks_status: method.checks_status(),
            uses_continuations: method
                .parameters()
                .iter()
                .any(NativeParameter::is_continuation),
        })
    }
}

struct NativeParameterView {
    name: Identifier,
    ty: TypeFragment,
}

impl NativeParameterView {
    fn from_parameter(parameter: &NativeParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            ty: parameter.ty(),
        }
    }
}

#[derive(Clone)]
struct ByteArrayParameterView {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
}

impl ByteArrayParameterView {
    fn from_bytes(parameter: &BytesParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            pointer: parameter.pointer().clone(),
            length: parameter.length().clone(),
        }
    }
}

#[derive(Clone)]
struct RecordParameterView {
    name: Identifier,
    c_type: Identifier,
    local: Identifier,
}

impl RecordParameterView {
    fn from_record(parameter: &RecordParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            c_type: parameter.c_type().clone(),
            local: parameter.local().clone(),
        }
    }
}

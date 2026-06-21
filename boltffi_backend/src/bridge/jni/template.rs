use askama::Template as AskamaTemplate;

use crate::{
    bridge::{
        c::{ArgumentList, Identifier, Literal, TypeFragment},
        jni::{BytesParameter, JniBridgeContract, NativeMethod, NativeParameter},
    },
    core::Result,
};

#[derive(AskamaTemplate)]
#[template(path = "bridge/jni/source.c", escape = "none")]
struct SourceFileTemplate {
    c_header: Literal,
    free_buffer: Identifier,
    checks_status: bool,
    uses_byte_arrays: bool,
    uses_exceptions: bool,
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
        Ok(SourceFileTemplate {
            c_header: Literal::string(contract.c_header().as_str()),
            free_buffer: contract.free_buffer().clone(),
            checks_status: methods.iter().any(|method| method.checks_status),
            uses_byte_arrays: methods
                .iter()
                .any(|method| method.returns_bytes || !method.byte_arrays.is_empty()),
            uses_exceptions: methods.iter().any(|method| {
                method.checks_status || method.returns_bytes || !method.byte_arrays.is_empty()
            }),
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
    arguments: ArgumentList,
    returns_void: bool,
    returns_boolean: bool,
    returns_bytes: bool,
    checks_status: bool,
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
            arguments: ArgumentList::from_iter(
                method
                    .parameters()
                    .iter()
                    .flat_map(NativeParameter::c_arguments),
            ),
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            returns_bytes: method.returns_bytes(),
            checks_status: method.checks_status(),
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

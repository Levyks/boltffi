use askama::Template as AskamaTemplate;

use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, Literal, TypeFragment},
        jni::{JniBridgeContract, NativeMethod, NativeParameter},
    },
    core::Result,
};

#[derive(AskamaTemplate)]
#[template(path = "bridge/jni/source.c", escape = "none")]
struct SourceFileTemplate {
    c_header: Literal,
    checks_status: bool,
    methods: Vec<NativeMethodView>,
}

struct NativeMethodView {
    symbol: Identifier,
    c_function: Identifier,
    return_type: TypeFragment,
    c_result_type: TypeFragment,
    parameters: Vec<NativeParameterView>,
    arguments: ArgumentList,
    returns_void: bool,
    returns_boolean: bool,
    checks_status: bool,
}

struct NativeParameterView {
    name: Identifier,
    ty: TypeFragment,
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
            checks_status: methods.iter().any(|method| method.checks_status),
            methods,
        }
        .render()?)
    }
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
            arguments: ArgumentList::from_iter(
                method
                    .parameters()
                    .iter()
                    .map(|parameter| Expression::identifier(parameter.name().clone())),
            ),
            returns_void: method.returns_void(),
            returns_boolean: method.returns_boolean(),
            checks_status: method.checks_status(),
        })
    }
}

impl NativeParameterView {
    fn from_parameter(parameter: &NativeParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            ty: parameter.ty().as_type_fragment(),
        }
    }
}

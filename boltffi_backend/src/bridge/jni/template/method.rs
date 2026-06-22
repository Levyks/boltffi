use crate::{
    bridge::{
        c::{ArgumentList, Expression, Identifier, TypeFragment},
        jni::{BytesParameter, NativeMethod, NativeParameter, RecordParameter},
    },
    core::Result,
};

pub struct NativeMethodView {
    pub symbol: Identifier,
    pub c_function: Identifier,
    pub return_type: TypeFragment,
    pub c_result_type: TypeFragment,
    pub parameters: Vec<NativeParameterView>,
    pub byte_arrays: Vec<ByteArrayParameterView>,
    pub record_arrays: Vec<RecordParameterView>,
    pub arguments: ArgumentList,
    pub returns_void: bool,
    pub returns_boolean: bool,
    pub returns_bytes: bool,
    pub returns_record: bool,
    pub returns_callback: bool,
    pub return_value: Expression,
    pub checks_status: bool,
    pub uses_continuations: bool,
}

pub struct NativeParameterView {
    pub name: Identifier,
    pub ty: TypeFragment,
}

#[derive(Clone)]
pub struct ByteArrayParameterView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
}

#[derive(Clone)]
pub struct RecordParameterView {
    pub name: Identifier,
    pub c_type: Identifier,
    pub local: Identifier,
}

impl NativeMethodView {
    pub fn from_method(method: &NativeMethod) -> Result<Self> {
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

impl NativeParameterView {
    fn from_parameter(parameter: &NativeParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            ty: parameter.ty(),
        }
    }
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

impl RecordParameterView {
    fn from_record(parameter: &RecordParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            c_type: parameter.c_type().clone(),
            local: parameter.local().clone(),
        }
    }
}

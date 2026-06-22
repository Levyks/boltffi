use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::{BytesParameter, JniType, RecordParameter},
    },
    core::Result,
};

/// JNI parameter accepted by one native method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeParameter {
    kind: NativeParameterKind,
}

impl NativeParameter {
    /// Returns the generated C parameter name.
    pub fn name(&self) -> &Identifier {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.name(),
            NativeParameterKind::Bytes(parameter) => parameter.name(),
            NativeParameterKind::Record(parameter) => parameter.name(),
        }
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.ty().as_type_fragment(),
            NativeParameterKind::Bytes(_) => TypeFragment::new("jbyteArray"),
            NativeParameterKind::Record(_) => TypeFragment::new("jbyteArray"),
        }
    }

    /// Returns C bridge call arguments produced from this JNI parameter.
    pub fn c_arguments(&self) -> Vec<Expression> {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => {
                vec![Expression::identifier(parameter.name().clone())]
            }
            NativeParameterKind::Bytes(parameter) => vec![
                Expression::cast(
                    TypeFragment::new("const uint8_t *"),
                    Expression::identifier(parameter.pointer().clone()),
                ),
                Expression::cast(
                    TypeFragment::new("uintptr_t"),
                    Expression::identifier(parameter.length().clone()),
                ),
            ],
            NativeParameterKind::Record(parameter) => {
                vec![Expression::identifier(parameter.local().clone())]
            }
        }
    }

    /// Returns byte-array parameter details when this parameter carries bytes.
    pub fn bytes(&self) -> Option<&BytesParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_) | NativeParameterKind::Record(_) => None,
            NativeParameterKind::Bytes(parameter) => Some(parameter),
        }
    }

    /// Returns direct-record parameter details when this parameter carries a record.
    pub fn record(&self) -> Option<&RecordParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_) | NativeParameterKind::Bytes(_) => None,
            NativeParameterKind::Record(parameter) => Some(parameter),
        }
    }

    /// Creates JNI parameters from C ABI parameters.
    pub fn from_c_parameters(parameters: &[c::Parameter]) -> Result<Vec<Self>> {
        let mut index = 0;
        std::iter::from_fn(|| {
            (index < parameters.len()).then(|| {
                let parameter = &parameters[index];
                match BytesParameter::from_pair(parameter, parameters.get(index + 1)) {
                    Ok(Some(bytes)) => {
                        index += 2;
                        Ok(Self {
                            kind: NativeParameterKind::Bytes(bytes),
                        })
                    }
                    Ok(None) => {
                        index += 1;
                        match RecordParameter::from_c_parameter(parameter)? {
                            Some(record) => Ok(Self {
                                kind: NativeParameterKind::Record(record),
                            }),
                            None => {
                                ScalarParameter::from_c_parameter(parameter).map(|scalar| Self {
                                    kind: NativeParameterKind::Scalar(scalar),
                                })
                            }
                        }
                    }
                    Err(error) => {
                        index = parameters.len();
                        Err(error)
                    }
                }
            })
        })
        .collect()
    }
}

/// JNI parameter shape selected from one or more C ABI parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum NativeParameterKind {
    /// A scalar JNI parameter passed directly to the C bridge.
    Scalar(ScalarParameter),
    /// A `jbyteArray` expanded to pointer and length C bridge arguments.
    Bytes(BytesParameter),
    /// A `jbyteArray` copied into one direct C record value.
    Record(RecordParameter),
}

/// Scalar JNI parameter mapped to one scalar C bridge argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ScalarParameter {
    name: Identifier,
    ty: JniType,
}

impl ScalarParameter {
    /// Returns the generated C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the scalar JNI parameter type.
    pub fn ty(&self) -> JniType {
        self.ty
    }

    /// Creates a scalar JNI parameter from one scalar C ABI parameter.
    pub fn from_c_parameter(parameter: &c::Parameter) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(parameter.name())?,
            ty: JniType::from_c_type(parameter.ty())?,
        })
    }
}

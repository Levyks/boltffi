use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::{
            BytesParameter, CallbackParameter, ClosureParameter, ClosureRegistration,
            ContinuationParameter, DirectVectorParameter, RecordParameter, ScalarParameter,
        },
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

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
            NativeParameterKind::DirectVector(parameter) => parameter.name(),
            NativeParameterKind::Record(parameter) => parameter.name(),
            NativeParameterKind::Callback(parameter) => parameter.name(),
            NativeParameterKind::Closure(parameter) => parameter.name(),
            NativeParameterKind::Continuation(parameter) => parameter.name(),
        }
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => parameter.ty().as_type_fragment(),
            NativeParameterKind::Bytes(_) => TypeFragment::new("jbyteArray"),
            NativeParameterKind::DirectVector(parameter) => parameter.array_type(),
            NativeParameterKind::Record(_) => TypeFragment::new("jbyteArray"),
            NativeParameterKind::Callback(parameter) => parameter.ty(),
            NativeParameterKind::Closure(parameter) => parameter.ty(),
            NativeParameterKind::Continuation(parameter) => parameter.ty(),
        }
    }

    /// Returns C bridge call arguments produced from this JNI parameter.
    pub fn c_arguments(&self) -> Result<Vec<Expression>> {
        match &self.kind {
            NativeParameterKind::Scalar(parameter) => {
                parameter.c_argument().map(|value| vec![value])
            }
            NativeParameterKind::Bytes(parameter) => Ok(vec![
                Expression::cast(
                    TypeFragment::new("const uint8_t *"),
                    Expression::identifier(parameter.pointer().clone()),
                ),
                Expression::cast(
                    TypeFragment::new("uintptr_t"),
                    Expression::identifier(parameter.length().clone()),
                ),
            ]),
            NativeParameterKind::DirectVector(parameter) => Ok(parameter.c_arguments()),
            NativeParameterKind::Record(parameter) => {
                Ok(vec![Expression::identifier(parameter.local().clone())])
            }
            NativeParameterKind::Callback(parameter) => Ok(vec![parameter.c_argument()]),
            NativeParameterKind::Closure(parameter) => Ok(parameter.c_arguments()),
            NativeParameterKind::Continuation(parameter) => parameter.c_arguments(),
        }
    }

    /// Returns byte-array parameter details when this parameter carries bytes.
    pub fn bytes(&self) -> Option<&BytesParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_)
            | NativeParameterKind::DirectVector(_)
            | NativeParameterKind::Record(_)
            | NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => None,
            NativeParameterKind::Bytes(parameter) => Some(parameter),
        }
    }

    /// Returns direct-record parameter details when this parameter carries a record.
    pub fn record(&self) -> Option<&RecordParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_)
            | NativeParameterKind::Bytes(_)
            | NativeParameterKind::DirectVector(_)
            | NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => None,
            NativeParameterKind::Record(parameter) => Some(parameter),
        }
    }

    /// Returns direct-vector parameter details when this parameter carries a Java array.
    pub fn direct_vector(&self) -> Option<&DirectVectorParameter> {
        match &self.kind {
            NativeParameterKind::Scalar(_)
            | NativeParameterKind::Bytes(_)
            | NativeParameterKind::Record(_)
            | NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => None,
            NativeParameterKind::DirectVector(parameter) => Some(parameter),
        }
    }

    /// Returns whether this parameter supplies a C poll continuation.
    pub fn is_continuation(&self) -> bool {
        matches!(self.kind, NativeParameterKind::Continuation(_))
    }

    /// Creates JNI parameters from C ABI parameter groups.
    pub fn from_c_function(
        function: &c::Function,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Vec<Self>> {
        function
            .parameter_groups()
            .iter()
            .map(|group| Self::from_c_group(function, group, callbacks, closures))
            .collect()
    }

    fn from_c_group(
        function: &c::Function,
        group: &c::ParameterGroup,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        match group {
            c::ParameterGroup::Value(index) => {
                let parameter = function.parameter(*index);
                match RecordParameter::from_c_parameter(parameter)? {
                    Some(record) => Ok(Self {
                        kind: NativeParameterKind::Record(record),
                    }),
                    None => match CallbackParameter::from_c_parameter(parameter, callbacks)? {
                        Some(callback) => Ok(Self {
                            kind: NativeParameterKind::Callback(callback),
                        }),
                        None => ScalarParameter::from_c_parameter(parameter).map(|scalar| Self {
                            kind: NativeParameterKind::Scalar(scalar),
                        }),
                    },
                }
            }
            c::ParameterGroup::ByteSlice(bytes) => {
                BytesParameter::from_c_group(bytes).map(|bytes| Self {
                    kind: NativeParameterKind::Bytes(bytes),
                })
            }
            c::ParameterGroup::DirectVector(vector) => {
                DirectVectorParameter::from_c_group(vector, function).map(|vector| Self {
                    kind: NativeParameterKind::DirectVector(vector),
                })
            }
            c::ParameterGroup::CallbackCompletion(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback completion parameter group cannot appear on a JNI native method",
            }),
            c::ParameterGroup::Continuation(continuation) => {
                ContinuationParameter::from_c_group(continuation, function).map(|continuation| {
                    Self {
                        kind: NativeParameterKind::Continuation(continuation),
                    }
                })
            }
            c::ParameterGroup::Closure(closure) => {
                ClosureParameter::from_c_group(closure, closures).map(|closure| Self {
                    kind: NativeParameterKind::Closure(closure),
                })
            }
        }
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
    /// A Java primitive array expanded to pointer and length C bridge arguments.
    DirectVector(DirectVectorParameter),
    /// A `jbyteArray` copied into one direct C record value.
    Record(RecordParameter),
    /// A `jlong` Java callback handle converted through a C callback constructor.
    Callback(CallbackParameter),
    /// A `jlong` Java closure handle expanded to call, context, and release C ABI parameters.
    Closure(ClosureParameter),
    /// A `jlong` callback data value paired with the generated JNI continuation callback.
    Continuation(ContinuationParameter),
}

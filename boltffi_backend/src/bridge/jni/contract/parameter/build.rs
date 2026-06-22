use crate::{
    bridge::{
        c,
        jni::{
            BytesParameter, CallbackParameter, ClosureParameter, ClosureRegistration,
            ContinuationParameter, DirectVectorParameter, NativeParameter, NativeParameterKind,
            RecordParameter, ScalarParameter,
        },
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

impl NativeParameter {
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
                Self::from_value_parameter(function.parameter(*index), callbacks)
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

    fn from_value_parameter(parameter: &c::Parameter, callbacks: &[c::Callback]) -> Result<Self> {
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
}

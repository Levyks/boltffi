//! Build pass from C ABI parameter groups to JNI native parameters.
//!
//! The C bridge contract groups low-level ABI pieces before this module sees
//! them. A byte slice is already pointer plus length, a direct vector is already
//! pointer plus count, a continuation is already callback plus context, and a
//! closure is already call, context, and release.
//!
//! This module turns each C group into the Java parameter that should appear in
//! the `Java_*` signature. It is the boundary that prevents method templates
//! from counting adjacent C parameters or guessing that three values are really
//! one closure.

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
            .collect::<Result<Vec<_>>>()
            .map(|parameters| parameters.into_iter().flatten().collect())
    }

    fn from_c_group(
        function: &c::Function,
        group: &c::ParameterGroup,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Option<Self>> {
        match group {
            c::ParameterGroup::Value(index) => {
                Self::from_value_parameter(function.parameter(*index), callbacks).map(Some)
            }
            c::ParameterGroup::ByteSlice(bytes) => BytesParameter::from_c_group(bytes)
                .map(|bytes| Some(Self::new(NativeParameterKind::Bytes(bytes)))),
            c::ParameterGroup::DirectVector(vector) => {
                DirectVectorParameter::from_c_group(vector, function)
                    .map(|vector| Some(Self::new(NativeParameterKind::DirectVector(vector))))
            }
            c::ParameterGroup::SuccessOut(_) => Ok(None),
            c::ParameterGroup::DirectWriteback(writeback) => {
                RecordParameter::from_c_writeback(writeback, function)
                    .map(|record| Some(Self::new(NativeParameterKind::Record(record))))
            }
            c::ParameterGroup::CallbackCompletion(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback completion parameter group cannot appear on a JNI native method",
            }),
            c::ParameterGroup::Continuation(continuation) => {
                ContinuationParameter::from_c_group(continuation, function).map(|continuation| {
                    Some(Self::new(NativeParameterKind::Continuation(continuation)))
                })
            }
            c::ParameterGroup::Closure(closure) => {
                ClosureParameter::from_c_group(closure, closures)
                    .map(|closure| Some(Self::new(NativeParameterKind::Closure(closure))))
            }
            c::ParameterGroup::ClosureReturn(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure return out-pointer cannot appear on a JNI native method",
            }),
        }
    }

    fn from_value_parameter(parameter: &c::Parameter, callbacks: &[c::Callback]) -> Result<Self> {
        match RecordParameter::from_c_parameter(parameter)? {
            Some(record) => Ok(Self::new(NativeParameterKind::Record(record))),
            None => match CallbackParameter::from_c_parameter(parameter, callbacks)? {
                Some(callback) => Ok(Self::new(NativeParameterKind::Callback(callback))),
                None => ScalarParameter::from_c_parameter(parameter)
                    .map(|scalar| Self::new(NativeParameterKind::Scalar(scalar))),
            },
        }
    }
}

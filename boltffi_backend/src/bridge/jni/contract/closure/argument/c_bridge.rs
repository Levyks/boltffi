use crate::{
    bridge::{
        c,
        jni::{CallbackClosureHandle, JvmClassPath},
    },
    core::{Error, Result},
};

use super::super::names::ClosureNames;
use super::{
    ClosureArgument, ClosureArgumentKind, ClosureBytesArgument, ClosureCParameter,
    ClosureDirectVectorArgument, ClosureHandleArgument, ClosureScalarArgument,
};

const JNI_BRIDGE: &str = "jni";

enum ClosureCall<'source> {
    Parameter(&'source c::ClosureParameter),
    Return(&'source c::ClosureReturnParameter),
}

impl ClosureArgument {
    /// Builds a closure-call argument from a closure parameter group.
    pub fn from_closure_group(
        class: &JvmClassPath,
        closure: &c::ClosureParameter,
        group: &c::ParameterGroup,
    ) -> Result<Self> {
        Self::from_group(class, ClosureCall::Parameter(closure), group)
    }

    /// Builds a closure-call argument from a closure return storage group.
    pub fn from_return_group(
        class: &JvmClassPath,
        returned: &c::ClosureReturnParameter,
        group: &c::ParameterGroup,
    ) -> Result<Self> {
        Self::from_group(class, ClosureCall::Return(returned), group)
    }

    fn from_group(
        class: &JvmClassPath,
        call: ClosureCall<'_>,
        group: &c::ParameterGroup,
    ) -> Result<Self> {
        match group {
            c::ParameterGroup::Value(index) => Ok(Self {
                kind: ClosureArgumentKind::Scalar(ClosureScalarArgument::from_parameter(
                    call.parameter(*index),
                )?),
            }),
            c::ParameterGroup::ByteSlice(bytes) => Ok(Self {
                kind: ClosureArgumentKind::Bytes(ClosureBytesArgument::from_bytes(
                    call.parameter(bytes.pointer()),
                    call.parameter(bytes.length()),
                    bytes,
                )?),
            }),
            c::ParameterGroup::DirectVector(vector) => Ok(Self {
                kind: ClosureArgumentKind::DirectVector(ClosureDirectVectorArgument::from_vector(
                    call.parameter(vector.pointer()),
                    call.parameter(vector.length()),
                    vector,
                )?),
            }),
            c::ParameterGroup::DirectWriteback(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure call argument cannot be a direct-record writeback",
            }),
            c::ParameterGroup::CallbackCompletion(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure call argument cannot be an async callback completion",
            }),
            c::ParameterGroup::Continuation(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure call argument cannot be a poll continuation",
            }),
            c::ParameterGroup::Closure(nested) => Ok(Self {
                kind: ClosureArgumentKind::Closure(Self::from_nested_closure(class, call, nested)?),
            }),
            c::ParameterGroup::ClosureReturn(_) => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure call argument cannot be a closure return out-pointer",
            }),
        }
    }

    fn from_nested_closure(
        class: &JvmClassPath,
        call: ClosureCall<'_>,
        nested: &c::ClosureParameter,
    ) -> Result<ClosureHandleArgument> {
        let names = ClosureNames::new(nested.signature());
        let handle = CallbackClosureHandle::new(
            class,
            nested.signature(),
            call.parameter(nested.call()).ty(),
        )?;
        ClosureHandleArgument::new(
            nested.name(),
            ClosureCParameter::from_parameter(call.parameter(nested.call()))?,
            ClosureCParameter::from_parameter(call.parameter(nested.context()))?,
            ClosureCParameter::from_parameter(call.parameter(nested.release()))?,
            &handle,
            names.call()?,
            names.release()?,
        )
    }
}

impl ClosureCall<'_> {
    fn parameter(&self, index: c::ParameterIndex) -> &c::Parameter {
        match self {
            Self::Parameter(closure) => closure.parameter(index),
            Self::Return(returned) => returned.parameter(index),
        }
    }
}

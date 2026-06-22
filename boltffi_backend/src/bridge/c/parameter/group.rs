use crate::core::{Error, Result};

use super::super::C_BRIDGE_CONTRACT;
use super::{
    ByteSliceParameter, CallbackCompletionParameter, ClosureParameter, ContinuationParameter,
    Parameter, ParameterIndex, ParameterRole,
};

/// Source-level parameter group represented by one or more C ABI parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParameterGroup {
    /// One source parameter maps to one C ABI parameter.
    Value(ParameterIndex),
    /// One source parameter maps to a borrowed byte pointer and byte length.
    ByteSlice(ByteSliceParameter),
    /// One async callback completion maps to callback and context parameters.
    CallbackCompletion(CallbackCompletionParameter),
    /// One poll continuation maps to callback data and a function pointer.
    Continuation(ContinuationParameter),
    /// One closure parameter maps to call, context, and release C ABI parameters.
    Closure(ClosureParameter),
}

impl ParameterGroup {
    /// Builds source-level parameter groups from flat C ABI parameters.
    pub fn from_params(params: &[Parameter]) -> Result<Vec<Self>> {
        let mut index = 0;
        std::iter::from_fn(|| {
            (index < params.len()).then(|| {
                let group = Self::from_param(params, index);
                index += group.as_ref().map_or(1, Self::width);
                group
            })
        })
        .collect()
    }

    fn from_param(params: &[Parameter], index: usize) -> Result<Self> {
        match &params[index].role {
            ParameterRole::Value => Ok(Self::Value(ParameterIndex::new(index))),
            ParameterRole::BytePointer(name) => {
                ByteSliceParameter::from_params(params, index, name).map(Self::ByteSlice)
            }
            ParameterRole::ByteLength(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "byte slice parameter group does not start with pointer parameter",
            }),
            ParameterRole::CallbackCompletionCallback(name) => {
                CallbackCompletionParameter::from_params(params, index, name)
                    .map(Self::CallbackCompletion)
            }
            ParameterRole::CallbackCompletionContext(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter group does not start with callback parameter",
            }),
            ParameterRole::ContinuationData(name) => {
                ContinuationParameter::from_params(params, index, name).map(Self::Continuation)
            }
            ParameterRole::ContinuationCallback(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "continuation parameter group does not start with data parameter",
            }),
            ParameterRole::ClosureCall { name, signature } => {
                ClosureParameter::from_params(params, index, name, signature).map(Self::Closure)
            }
            ParameterRole::ClosureContext(_) | ParameterRole::ClosureRelease(_) => {
                Err(Error::BrokenBridgeContract {
                    bridge: C_BRIDGE_CONTRACT,
                    invariant: "closure parameter group does not start with call parameter",
                })
            }
        }
    }

    fn width(&self) -> usize {
        match self {
            Self::Value(_) => 1,
            Self::ByteSlice(_) => 2,
            Self::CallbackCompletion(_) => 2,
            Self::Continuation(_) => 2,
            Self::Closure(_) => 3,
        }
    }
}

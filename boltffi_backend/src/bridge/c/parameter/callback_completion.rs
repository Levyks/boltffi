use crate::core::{Error, Result};

use super::super::{C_BRIDGE_CONTRACT, Identifier};
use super::{Parameter, ParameterIndex};

/// C ABI parameters that carry one async callback completion.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionParameter {
    name: Identifier,
    callback: ParameterIndex,
    context: ParameterIndex,
}

impl CallbackCompletionParameter {
    /// Returns the source completion name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the callback function pointer parameter position.
    pub const fn callback(&self) -> ParameterIndex {
        self.callback
    }

    /// Returns the callback context parameter position.
    pub const fn context(&self) -> ParameterIndex {
        self.context
    }

    pub(in crate::bridge::c::parameter) fn from_params(
        params: &[Parameter],
        callback: usize,
        name: &Identifier,
    ) -> Result<Self> {
        let context = callback + 1;
        let context_role = params.get(context).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter group is missing context parameter",
            },
        )?;

        if !context_role.is_callback_completion_context(name) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "callback completion parameter group has mismatched context parameter",
            });
        }

        Ok(Self {
            name: name.clone(),
            callback: ParameterIndex::new(callback),
            context: ParameterIndex::new(context),
        })
    }
}

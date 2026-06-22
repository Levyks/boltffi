use crate::core::{Error, Result};

use boltffi_binding::ClosureSignature;

use super::super::{C_BRIDGE_CONTRACT, Identifier};
use super::{Parameter, ParameterIndex};

/// C ABI parameters that carry one closure argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureParameter {
    name: Identifier,
    signature: ClosureSignature,
    call: ParameterIndex,
    context: ParameterIndex,
    release: ParameterIndex,
}

impl ClosureParameter {
    /// Returns the source parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the closure invocation signature.
    pub fn signature(&self) -> &ClosureSignature {
        &self.signature
    }

    /// Returns the call function pointer parameter position.
    pub const fn call(&self) -> ParameterIndex {
        self.call
    }

    /// Returns the callback context parameter position.
    pub const fn context(&self) -> ParameterIndex {
        self.context
    }

    /// Returns the callback release function parameter position.
    pub const fn release(&self) -> ParameterIndex {
        self.release
    }

    pub(in crate::bridge::c::parameter) fn from_params(
        params: &[Parameter],
        call: usize,
        name: &Identifier,
        signature: &ClosureSignature,
    ) -> Result<Self> {
        let context = call + 1;
        let release = call + 2;
        let context_role = params.get(context).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure parameter group is missing context parameter",
            },
        )?;
        let release_role = params.get(release).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure parameter group is missing release parameter",
            },
        )?;

        if !context_role.is_closure_context(name) || !release_role.is_closure_release(name) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure parameter group has mismatched context or release parameter",
            });
        }

        Ok(Self {
            name: name.clone(),
            signature: signature.clone(),
            call: ParameterIndex::new(call),
            context: ParameterIndex::new(context),
            release: ParameterIndex::new(release),
        })
    }
}

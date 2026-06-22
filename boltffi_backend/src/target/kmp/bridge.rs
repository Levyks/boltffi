use boltffi_binding::{Bindings, Native};

use crate::core::{
    BridgeCapabilities, BridgeContract, GeneratedOutput, Result, bridge, contract::sealed,
};

use super::Syntax;

/// No-op bridge used while the KMP IR backend skeleton is being introduced.
///
/// Later milestones will replace this bridge contract with the concrete ABI
/// model needed by the KMP lowerer. Keeping it explicit lets
/// [`crate::Target`] exercise the same typed host/bridge composition path as
/// the Python backend without implying C, JNI, or Apple output ownership.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpBridge;

/// Bridge contract consumed by the skeletal KMP host.
///
/// The contract advertises no bridge capabilities because M1a does not render
/// target-language calls over an ABI yet. Host-side admission remains strict:
/// exported declarations are rejected before rendering until M1b introduces
/// the real KMP plan and support report.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpBridgeContract {
    capabilities: BridgeCapabilities,
}

impl KmpBridgeContract {
    fn empty() -> Self {
        Self {
            capabilities: BridgeCapabilities::new(),
        }
    }
}

impl bridge::BridgeBackend for KmpBridge {
    type Surface = Native;
    type Input = Bindings<Native>;
    type Contract = KmpBridgeContract;
    type Syntax = Syntax;

    fn build_contract(&self, _input: &Self::Input) -> Result<Self::Contract> {
        Ok(KmpBridgeContract::empty())
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        _contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        Ok(GeneratedOutput::empty())
    }
}

impl sealed::BridgeBackend for KmpBridge {}

impl BridgeContract for KmpBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for KmpBridgeContract {}

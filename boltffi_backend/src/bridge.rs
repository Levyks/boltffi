//! Bridge renderers and bridge stack composition.
//!
//! A bridge converts classified bindings into the ABI contract a host
//! language renderer consumes. Base bridges consume
//! [`boltffi_binding::Bindings`] directly; layered bridges consume the
//! contract produced by the layer below them. The [`Layer`] type keeps
//! that dependency in the type system.

use boltffi_binding::{Bindings, Surface};

use crate::{BridgeCapability, CapabilitySet, Emitted, Result, sealed};

/// Contract produced by a bridge stack.
pub trait Contract: sealed::BridgeContract {
    /// Binding surface this bridge contract serves.
    type Surface: Surface;

    /// Returns the bridge capabilities this contract provides.
    fn capabilities(&self) -> &CapabilitySet<BridgeCapability>;
}

/// Renderer for one bridge layer.
pub trait Renderer: sealed::BridgeRenderer {
    /// Binding surface this bridge layer serves.
    type Surface: Surface;
    /// Input value consumed by this bridge layer.
    type Input;
    /// Contract produced for layers or hosts above this bridge.
    type Contract: Contract<Surface = Self::Surface>;

    /// Builds the bridge contract from the input value.
    fn contract(&self, input: &Self::Input) -> Result<Self::Contract>;

    /// Renders files owned by this bridge layer.
    fn render(&self, input: &Self::Input, contract: &Self::Contract) -> Result<Emitted>;
}

/// A fully composed bridge stack.
pub trait Stack: sealed::BridgeStack {
    /// Binding surface this bridge stack serves.
    type Surface: Surface;
    /// Topmost bridge contract this stack exposes to a host.
    type Contract: Contract<Surface = Self::Surface>;

    /// Builds the bridge stack for a binding contract.
    fn build(&self, bindings: &Bindings<Self::Surface>) -> Result<Output<Self::Contract>>;
}

/// Output of a bridge stack build.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Output<C> {
    contract: C,
    emitted: Emitted,
}

impl<C> Output<C> {
    /// Creates bridge stack output.
    pub fn new(contract: C, emitted: Emitted) -> Self {
        Self { contract, emitted }
    }

    /// Returns the produced bridge contract.
    pub const fn contract(&self) -> &C {
        &self.contract
    }

    /// Returns files emitted by bridge layers.
    pub const fn emitted(&self) -> &Emitted {
        &self.emitted
    }

    /// Splits this output into contract and rendered fragments.
    pub fn into_parts(self) -> (C, Emitted) {
        (self.contract, self.emitted)
    }
}

/// A bridge layer stacked above another bridge stack.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Layer<Lower, Upper> {
    lower: Lower,
    upper: Upper,
}

impl<Lower, Upper> Layer<Lower, Upper> {
    /// Creates a layered bridge stack.
    pub const fn new(lower: Lower, upper: Upper) -> Self {
        Self { lower, upper }
    }

    /// Returns the lower bridge stack.
    pub const fn lower(&self) -> &Lower {
        &self.lower
    }

    /// Returns the upper bridge layer.
    pub const fn upper(&self) -> &Upper {
        &self.upper
    }
}

impl<B, S> sealed::BridgeStack for B
where
    B: Renderer<Input = Bindings<S>, Surface = S>,
    S: Surface,
{
}

impl<B, S> Stack for B
where
    B: Renderer<Input = Bindings<S>, Surface = S>,
    S: Surface,
{
    type Surface = S;
    type Contract = B::Contract;

    fn build(&self, bindings: &Bindings<Self::Surface>) -> Result<Output<Self::Contract>> {
        let contract = self.contract(bindings)?;
        let emitted = self.render(bindings, &contract)?;
        Ok(Output::new(contract, emitted))
    }
}

impl<Lower, Upper> sealed::BridgeStack for Layer<Lower, Upper>
where
    Lower: Stack,
    Upper: Renderer<Input = Lower::Contract, Surface = Lower::Surface>,
{
}

impl<Lower, Upper> Stack for Layer<Lower, Upper>
where
    Lower: Stack,
    Upper: Renderer<Input = Lower::Contract, Surface = Lower::Surface>,
{
    type Surface = Lower::Surface;
    type Contract = Upper::Contract;

    fn build(&self, bindings: &Bindings<Self::Surface>) -> Result<Output<Self::Contract>> {
        let lower = self.lower.build(bindings)?;
        let (lower_contract, mut emitted) = lower.into_parts();
        let contract = self.upper.contract(&lower_contract)?;
        emitted.append(self.upper.render(&lower_contract, &contract)?);
        Ok(Output::new(contract, emitted))
    }
}

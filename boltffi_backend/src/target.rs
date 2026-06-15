use boltffi_binding::Bindings;

use crate::{
    CapabilityRequirements, Emitted, File, RenderContext, Result,
    bridge::{self, Contract},
    host,
};

/// A host renderer paired with the bridge stack it requires.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct Target<H, S> {
    host: H,
    stack: S,
}

impl<H, S> Target<H, S>
where
    H: host::Renderer<Contract = S::Contract, Surface = S::Surface>,
    S: bridge::Stack,
{
    /// Creates a target from a host renderer and bridge stack.
    pub const fn new(host: H, stack: S) -> Self {
        Self { host, stack }
    }

    /// Returns the host renderer.
    pub const fn host(&self) -> &H {
        &self.host
    }

    /// Returns the bridge stack.
    pub const fn stack(&self) -> &S {
        &self.stack
    }

    /// Renders a binding contract through the paired bridge and host.
    pub fn render(&self, bindings: &Bindings<S::Surface>) -> Result<Vec<File>> {
        let bridge = self.stack.build(bindings)?;
        let (contract, bridge_emitted) = bridge.into_parts();
        let binding_requirements =
            CapabilityRequirements::from_bindings(self.host.name(), bindings)?;
        self.host
            .binding_capabilities()
            .require_binding(self.host.name(), &binding_requirements)?;
        contract
            .capabilities()
            .require_bridge(self.host.name(), &self.host.bridge_requirements())?;
        let context = RenderContext::new(bindings, self.host.name());
        let host_emitted = bindings
            .decls()
            .iter()
            .map(|decl| host::render_decl(&self.host, decl, &contract, &context))
            .collect::<Result<Vec<_>>>()
            .map(Emitted::combine)?;
        let mut emitted = bridge_emitted;
        emitted.append(host_emitted);
        self.host.files(bindings, emitted)
    }
}

#[cfg(test)]
mod tests {
    use boltffi_binding::{Bindings, Native};

    use crate::{
        BindingCapability, BridgeCapability, CapabilityRequirements, CapabilitySet, Emitted, File,
        RenderContext, Result, bridge, host, sealed,
    };

    #[derive(Clone)]
    struct NativeContract {
        capabilities: CapabilitySet<BridgeCapability>,
    }

    impl sealed::BridgeContract for NativeContract {}

    impl bridge::Contract for NativeContract {
        type Surface = Native;

        fn capabilities(&self) -> &CapabilitySet<BridgeCapability> {
            &self.capabilities
        }
    }

    #[derive(Clone, Copy)]
    struct NativeBridge;

    impl sealed::BridgeRenderer for NativeBridge {}

    impl bridge::Renderer for NativeBridge {
        type Surface = Native;
        type Input = Bindings<Native>;
        type Contract = NativeContract;

        fn contract(&self, _input: &Self::Input) -> Result<Self::Contract> {
            Ok(NativeContract {
                capabilities: CapabilitySet::new().stable(BridgeCapability::CAbi),
            })
        }

        fn render(&self, _input: &Self::Input, _contract: &Self::Contract) -> Result<Emitted> {
            Ok(Emitted::empty())
        }
    }

    #[derive(Clone, Copy)]
    struct SwiftHost;

    impl sealed::HostRenderer for SwiftHost {}

    impl host::Renderer for SwiftHost {
        type Surface = Native;
        type Contract = NativeContract;

        fn name(&self) -> &'static str {
            "swift"
        }

        fn binding_capabilities(&self) -> CapabilitySet<BindingCapability> {
            CapabilitySet::new()
        }

        fn bridge_requirements(&self) -> CapabilityRequirements<BridgeCapability> {
            CapabilityRequirements::new().require(BridgeCapability::CAbi)
        }

        fn record(
            &self,
            _decl: &boltffi_binding::RecordDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn enumeration(
            &self,
            _decl: &boltffi_binding::EnumDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn function(
            &self,
            _decl: &boltffi_binding::FunctionDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn class(
            &self,
            _decl: &boltffi_binding::ClassDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn callback(
            &self,
            _decl: &boltffi_binding::CallbackDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn stream(
            &self,
            _decl: &boltffi_binding::StreamDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn constant(
            &self,
            _decl: &boltffi_binding::ConstantDecl<Self::Surface>,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn custom_type(
            &self,
            _decl: &boltffi_binding::CustomTypeDecl,
            _bridge: &Self::Contract,
            _context: &RenderContext<Self::Surface>,
        ) -> Result<Emitted> {
            Ok(Emitted::empty())
        }

        fn files(
            &self,
            _bindings: &Bindings<Self::Surface>,
            emitted: Emitted,
        ) -> Result<Vec<File>> {
            Ok(File::assemble(emitted.into_parts().0))
        }
    }

    #[test]
    fn target_accepts_host_with_matching_bridge_contract() {
        let _target = super::Target::new(SwiftHost, NativeBridge);
    }
}

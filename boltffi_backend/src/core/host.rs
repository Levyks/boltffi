//! Host-language renderers.
//!
//! A host renderer emits target-language source over one bridge
//! contract. Its associated [`HostBackend::Bridge`] type is the only
//! bridge shape it can receive, and [`crate::Target`] uses that
//! associated type to reject invalid host/bridge pairings at compile
//! time.

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Surface,
};

use crate::core::{
    BridgeCapability, BridgeContract, CapabilityRequirements, Emitted, FileGroup, FileLayout,
    HostCapabilities, RenderContext, Result, contract::sealed,
};

/// Host renderer for one target language.
#[allow(private_bounds)]
pub trait HostBackend: sealed::HostBackend {
    /// Binding surface this host renders.
    type Surface: Surface;
    /// Bridge contract this host accepts.
    type Bridge: BridgeContract<Surface = Self::Surface>;
    /// File grouping rule used by this host layout.
    type Files: FileGroup;

    /// Returns the target name used in diagnostics.
    fn name(&self) -> &'static str;

    /// Returns binding capabilities this host can render.
    fn binding_capabilities(&self) -> HostCapabilities;

    /// Returns bridge capabilities this host requires.
    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability>;

    /// Renders a record declaration.
    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders an enum declaration.
    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a free function declaration.
    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a class declaration.
    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a callback trait declaration.
    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a stream declaration.
    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a constant declaration.
    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a custom type declaration.
    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Returns the file layout for this host.
    fn file_layout(&self, bindings: &Bindings<Self::Surface>) -> FileLayout<Self::Files>;
}

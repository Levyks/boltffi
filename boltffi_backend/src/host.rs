//! Host-language renderers.
//!
//! A host renderer emits target-language source over one bridge
//! contract. Its associated [`Renderer::Contract`] type is the only
//! bridge shape it can receive, and [`crate::Target`] uses that
//! associated type to reject invalid host/bridge pairings at compile
//! time.

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, Decl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Surface,
};

use crate::{
    BindingCapability, BridgeCapability, CapabilityRequirements, CapabilitySet, Emitted, File,
    RenderContext, Result, bridge, sealed,
};

/// Renderer for a target-language host.
pub trait Renderer: sealed::HostRenderer {
    /// Binding surface this host renders.
    type Surface: Surface;
    /// Bridge contract this host accepts.
    type Contract: bridge::Contract<Surface = Self::Surface>;

    /// Returns the target name used in diagnostics.
    fn name(&self) -> &'static str;

    /// Returns binding capabilities this host can render.
    fn binding_capabilities(&self) -> CapabilitySet<BindingCapability>;

    /// Returns bridge capabilities this host requires.
    fn bridge_requirements(&self) -> CapabilityRequirements<BridgeCapability>;

    /// Renders a record declaration.
    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders an enum declaration.
    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a free function declaration.
    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a class declaration.
    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a callback trait declaration.
    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a stream declaration.
    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a constant declaration.
    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Renders a custom type declaration.
    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        bridge: &Self::Contract,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted>;

    /// Assembles rendered fragments into files for this host.
    fn files(&self, bindings: &Bindings<Self::Surface>, emitted: Emitted) -> Result<Vec<File>>;
}

pub(crate) fn render_decl<H>(
    host: &H,
    decl: &Decl<H::Surface>,
    bridge: &H::Contract,
    context: &RenderContext<H::Surface>,
) -> Result<Emitted>
where
    H: Renderer,
{
    match decl {
        Decl::Record(record) => host.record(record, bridge, context),
        Decl::Enum(enumeration) => host.enumeration(enumeration, bridge, context),
        Decl::Function(function) => host.function(function, bridge, context),
        Decl::Class(class) => host.class(class, bridge, context),
        Decl::Callback(callback) => host.callback(callback, bridge, context),
        Decl::Stream(stream) => host.stream(stream, bridge, context),
        Decl::Constant(constant) => host.constant(constant, bridge, context),
        Decl::CustomType(custom_type) => host.custom_type(custom_type, bridge, context),
        _ => Err(crate::Error::UnsupportedDeclaration {
            target: context.target(),
            declaration: decl.id(),
        }),
    }
}

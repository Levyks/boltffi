//! Kotlin target rendered through the JNI bridge.

mod codec;
mod name_style;
mod render;
mod syntax;

use std::path::PathBuf;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::{
        c::CBridge,
        jni::{JniBridge, JniBridgeContract},
    },
    core::{
        BindingCapability, BridgeCapability, BridgeLayer, CapabilityRequirements, Emitted,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
        contract::sealed, host,
    },
};

pub use name_style::{KotlinFile, KotlinPackage};
use syntax::Syntax;

/// Kotlin host renderer for a generated JNI owner class.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KotlinHost {
    package: KotlinPackage,
    file: KotlinFile,
    c_header: PathBuf,
    jni_source: PathBuf,
}

impl KotlinHost {
    /// Creates a Kotlin host renderer.
    pub fn new(package: impl Into<String>, file: impl Into<String>) -> Result<Self> {
        Ok(Self {
            package: KotlinPackage::parse(package)?,
            file: KotlinFile::parse(file)?,
            c_header: PathBuf::from("jni/boltffi.h"),
            jni_source: PathBuf::from("jni/jni_glue.c"),
        })
    }

    /// Selects the generated C header path.
    pub fn c_header(mut self, path: impl Into<PathBuf>) -> Self {
        self.c_header = path.into();
        self
    }

    /// Selects the generated JNI source path.
    pub fn jni_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.jni_source = path.into();
        self
    }

    /// Creates the backend target stack for this Kotlin host.
    pub fn into_target(self) -> Result<Target<Self, BridgeLayer<CBridge, JniBridge>>> {
        Ok(Target::new(
            self.clone(),
            BridgeLayer::new(
                CBridge::new(self.c_header.clone())?,
                JniBridge::new(self.package.as_str(), "Native", self.jni_source.clone())?,
            ),
        ))
    }

    /// Returns the Kotlin package name.
    pub fn package(&self) -> &KotlinPackage {
        &self.package
    }

    /// Returns the generated Kotlin file name.
    pub fn file(&self) -> &KotlinFile {
        &self.file
    }
}

impl host::HostBackend for KotlinHost {
    type Surface = Native;
    type Bridge = JniBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "kotlin"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::Jni)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(decl)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration(decl)?.render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, context)?.render()
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::diagnostic(crate::core::Diagnostic::new(
            "kotlin target skipped class declaration",
        )))
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::diagnostic(crate::core::Diagnostic::new(
            "kotlin target skipped callback declaration",
        )))
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::diagnostic(crate::core::Diagnostic::new(
            "kotlin target skipped stream declaration",
        )))
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::diagnostic(crate::core::Diagnostic::new(
            "kotlin target skipped constant declaration",
        )))
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(Emitted::diagnostic(crate::core::Diagnostic::new(
            "kotlin target skipped custom type declaration",
        )))
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        render::Module::new(self, bridge, declarations).render()
    }
}

impl sealed::HostBackend for KotlinHost {}

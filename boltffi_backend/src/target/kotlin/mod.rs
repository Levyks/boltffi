//! Kotlin target rendered through the JNI bridge.

mod codec;
mod name_style;
mod primitive;
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

pub use name_style::{KotlinFile, KotlinLibrary, KotlinPackage};
use syntax::Syntax;

/// Desktop native-library loading policy for the generated Kotlin module.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum KotlinDesktopLoader {
    /// Load bundled native resources first, then fall back to the system loader.
    #[default]
    Bundled,
    /// Load the desktop fallback library through `System.loadLibrary`.
    System,
    /// Do not emit a desktop native-library load path.
    None,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct NativeLibraries {
    android: KotlinLibrary,
    desktop_jni: KotlinLibrary,
    desktop_fallback: KotlinLibrary,
    desktop_loader: KotlinDesktopLoader,
}

/// Kotlin host renderer for a generated JNI owner class.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KotlinHost {
    package: KotlinPackage,
    file: KotlinFile,
    c_header: PathBuf,
    jni_source: PathBuf,
    native_libraries: NativeLibraries,
}

impl KotlinHost {
    const TARGET: &'static str = "kotlin";

    /// Creates a Kotlin host renderer.
    pub fn new(package: impl Into<String>, file: impl Into<String>) -> Result<Self> {
        Ok(Self {
            package: KotlinPackage::parse(package)?,
            file: KotlinFile::parse(file)?,
            c_header: PathBuf::from("jni/boltffi.h"),
            jni_source: PathBuf::from("jni/jni_glue.c"),
            native_libraries: NativeLibraries::default()?,
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

    /// Selects the Android native library load name.
    pub fn android_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries.android = KotlinLibrary::parse(library)?;
        Ok(self)
    }

    /// Selects the desktop JNI wrapper library load name.
    pub fn desktop_jni_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries.desktop_jni = KotlinLibrary::parse(library)?;
        Ok(self)
    }

    /// Selects the desktop fallback library load name.
    pub fn desktop_fallback_library(mut self, library: impl Into<String>) -> Result<Self> {
        self.native_libraries.desktop_fallback = KotlinLibrary::parse(library)?;
        Ok(self)
    }

    /// Selects the desktop native-library loading policy.
    pub fn desktop_loader(mut self, loader: KotlinDesktopLoader) -> Self {
        self.native_libraries.desktop_loader = loader;
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

    fn native_libraries(&self) -> &NativeLibraries {
        &self.native_libraries
    }
}

impl NativeLibraries {
    fn default() -> Result<Self> {
        Ok(Self {
            android: KotlinLibrary::parse("boltffi")?,
            desktop_jni: KotlinLibrary::parse("boltffi_jni")?,
            desktop_fallback: KotlinLibrary::parse("boltffi")?,
            desktop_loader: KotlinDesktopLoader::default(),
        })
    }

    fn android(&self) -> &KotlinLibrary {
        &self.android
    }

    fn desktop_jni(&self) -> &KotlinLibrary {
        &self.desktop_jni
    }

    fn desktop_fallback(&self) -> &KotlinLibrary {
        &self.desktop_fallback
    }

    fn bundled_desktop_loader(&self) -> bool {
        matches!(self.desktop_loader, KotlinDesktopLoader::Bundled)
    }

    fn system_desktop_loader(&self) -> bool {
        matches!(self.desktop_loader, KotlinDesktopLoader::System)
    }
}

impl host::HostBackend for KotlinHost {
    type Surface = Native;
    type Bridge = JniBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        Self::TARGET
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::Jni)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Record::from_declaration(decl, bridge, context)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Enumeration::from_declaration_with_package(
            decl,
            bridge,
            context,
            Some(self.package()),
        )?
        .render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Function::from_declaration(decl, bridge, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Class::from_declaration(decl, bridge, context)?.render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Callback::from_declaration(decl, bridge, context)?.render()
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Stream::from_declaration(decl, context)?.render()
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Constant::from_declaration(decl, bridge, context)?.render()
    }

    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::CustomType::from_declaration(decl, context)?.render()
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        render::Module::new(self, bridge, context, declarations).render()
    }
}

impl sealed::HostBackend for KotlinHost {}

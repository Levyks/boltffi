//! Dart web target: js_interop bindings over the wasm/TypeScript backend's
//! already-generated JS module. See render.rs for the design rationale.

mod name_style;
mod render;
mod syntax;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    RecordDecl, StreamDecl, Wasm32,
};

use crate::{
    bridge::wasm::{WasmBridge, WasmBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error, FilePath,
        GeneratedFile, GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration,
        Result, Target, contract::sealed, host,
    },
};

use syntax::Syntax;

/// Dart-web host renderer configuration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct DartWebHost {
    package: String,
    /// Global property path the companion JS loader is expected to publish
    /// the compiled wasm/TS module's exports under (see runtime.dart.txt's
    /// module docs and the generated loader snippet).
    js_module: String,
}

impl DartWebHost {
    const TARGET: &'static str = "dart_web";

    /// Creates a Dart-web package renderer for the given pub package name.
    pub fn new(package: impl Into<String>) -> Result<Self> {
        let package = package.into();
        if package.is_empty() {
            return Err(Error::UnsupportedTarget {
                target: Self::TARGET,
                shape: "empty package name",
            });
        }
        let js_module = format!("__boltffi_{package}");
        Ok(Self { package, js_module })
    }

    /// Creates the metadata-backed Dart-web target.
    pub fn into_target(self) -> Target<Self, WasmBridge> {
        Target::new(self, WasmBridge)
    }
}

impl host::HostBackend for DartWebHost {
    type Surface = Wasm32;
    type Bridge = WasmBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        Self::TARGET
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::Wasm)
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::record(decl, context)
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::enumeration(decl, context)
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::function(decl, &self.js_module, context)
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::class(decl, &self.js_module, context)
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::callback(decl, context)
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: Self::TARGET,
            shape: "streams",
        })
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: Self::TARGET,
            shape: "constants",
        })
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: Self::TARGET,
            shape: "custom types",
        })
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let mut body = String::new();
        for declaration in declarations {
            let (_, emitted) = declaration.into_parts();
            body.push_str(emitted.primary_chunk().as_str());
        }
        let runtime = include_str!("runtime.dart.txt");
        let library = format!("{runtime}\n\n{body}");
        let pubspec = format!(
            "name: {}\n\nenvironment:\n  sdk: ^3.10.8\n\ndependencies:\n  async: ^2.13.0\n",
            self.package
        );
        let loader = format!(
            "// Companion JS loader: import this module (as a plain <script type=\"module\">\n// on web, or via a preloaded import in a Node harness) BEFORE the compiled\n// Dart-web app runs, so its `@JS('{js_module}.*')` bindings resolve.\nimport * as bindings from './{package}.js';\nglobalThis.{js_module} = bindings;\n",
            js_module = self.js_module,
            package = self.package
        );
        Ok(GeneratedOutput::new(
            vec![
                GeneratedFile::new(
                    FilePath::new(format!("lib/{}.dart", self.package))?,
                    library,
                ),
                GeneratedFile::new(FilePath::new("pubspec.yaml")?, pubspec),
                GeneratedFile::new(
                    FilePath::new(format!("{}_web_loader.mjs", self.package))?,
                    loader,
                ),
            ],
            Vec::new(),
        ))
    }
}

impl sealed::HostBackend for DartWebHost {}

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
    /// module docs and the generated loader snippet). The loader imports
    /// the ACTUAL target::typescript-generated module (type-stripped via
    /// Node's `stripTypeScriptTypes`, not hand-marshalled) -- see
    /// `boltffi_cli::pack::dart_web`, which produces that module and the
    /// vendored `@boltffi/runtime` alongside this package's `lib/src/web/`.
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
        // `{js_module}_ready` is a promise the app's own `web/index.html`
        // is expected to have already started (see the loader's doc
        // comment below, and the generated README next to it) -- this
        // function only awaits it, it never starts loading anything
        // itself. Keeping loading eager and script-tag-driven (rather than
        // triggered lazily from Dart) lets the browser fetch the wasm
        // module in parallel with the Dart runtime's own startup instead
        // of serialized behind it.
        let ready_fn = format!(
            r#"
@$$js.JS('{js_module}_ready')
external $$js.JSPromise<$$js.JSAny?>? get _$$readyPromise;

/// Waits for the WASM module to finish loading. Call (and await) this
/// before using any other export in this library.
///
/// Requires a `<script type="module">` block in your app's
/// `web/index.html` that starts loading the module and stores the
/// resulting promise on `window.{js_module}_ready` -- see this package's
/// generated README for the exact snippet to paste in.
Future<void> ensureInitialized() async {{
  final promise = _$$readyPromise;
  if (promise == null) {{
    throw StateError(
      "window.{js_module}_ready is not set. Add the <script "
      "type=\"module\"> block from this package's README to your app's "
      "web/index.html, before the Flutter/Dart script tag.",
    );
  }}
  await promise.toDart;
}}
"#,
            js_module = self.js_module,
        );
        let library = format!("{runtime}\n\n{body}{ready_fn}");
        let pubspec = format!(
            "name: {}\n\nenvironment:\n  sdk: ^3.10.8\n\ndependencies:\n  async: ^2.13.0\n",
            self.package
        );
        // The compiled module (`{module}.js`, produced by `boltffi pack
        // dart-web` via target::typescript + Node's `stripTypeScriptTypes`)
        // is expected to sit next to this loader, exporting a `default`
        // async `init(source)` plus one plain export per declaration --
        // exactly what `render.rs`'s `@JS('{js_module}.*')` bindings
        // already assume. `{module}_bg.wasm` is the wasm binary copied
        // alongside it by the same pack step.
        let loader = format!(
            r#"// ES module loader for Dart Web / Flutter WASM targets. Call
// `initBoltFFI(wasmUrlOrBytes)` before or during Dart app initialization so
// its `@JS('{js_module}.*')` bindings resolve. `{module}.js` (and the
// vendored `boltffi_runtime/` it imports) are produced by `boltffi pack
// dart-web`, not by `boltffi generate` -- run that first.
import * as __boltffiBindings from './{module}.js';

export async function initBoltFFI(wasmSource) {{
  const source = wasmSource ?? new URL('./{module}_bg.wasm', import.meta.url);
  // The compiled module's `init()` (-> `instantiateBoltFFI`) only accepts a
  // `Response` or a `BufferSource`, not a bare URL/string -- fetch it here
  // so callers can still pass either a URL/path or pre-fetched bytes.
  const resolved =
    typeof source === 'string' || source instanceof URL ? await fetch(source) : source;
  await __boltffiBindings.default(resolved);
  globalThis.{js_module} = __boltffiBindings;
  return globalThis.{js_module};
}}
"#,
            js_module = self.js_module,
            module = self.package,
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

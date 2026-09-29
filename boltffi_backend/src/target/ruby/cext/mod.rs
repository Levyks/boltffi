//! Ruby C extension host (experimental).
//!
//! The Ruby target renders a C extension that links the Rust static library
//! and calls the shared C ABI (`CBridge`) directly. Every value will cross as
//! a plain Ruby object that the extension builds eagerly, so Ruby code, and
//! YJIT, see ordinary Ruby values.
//!
//! The target renders the gem and an extension that defines the Ruby module.
//! It renders no declarations yet.

mod render;
mod runtime;
mod support;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, GeneratedOutput,
        HostCapabilities, RenderContext, RenderedDeclaration, Result, Target, contract::sealed,
        host,
    },
};

use self::{
    render::{extension, package::Package},
    support::unsupported,
};
use super::{
    name_style::{default_gem, default_module, extension_stem, package_snake},
    syntax::ConstantPath,
};

/// Ruby C extension host, paired with the shared C ABI bridge.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RubyCExtHost {
    module: Option<ConstantPath>,
    gem: Option<String>,
    version: Option<String>,
    library: Option<String>,
    cargo_manifest: Option<String>,
    active_features: String,
    feature_args: Vec<String>,
}

impl RubyCExtHost {
    /// Creates a Ruby host renderer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Selects the Ruby module that holds the functions and records, such as
    /// `MyLib::Native`.
    pub fn module_name(mut self, module: &str) -> Result<Self> {
        self.module = Some(ConstantPath::parse(module)?);
        Ok(self)
    }

    /// Selects the gem name. The extension stem is the gem name in
    /// `snake_case`.
    pub fn gem_name(mut self, gem: impl Into<String>) -> Self {
        self.gem = Some(gem.into());
        self
    }

    /// Selects the gem version.
    pub fn version(mut self, version: Option<String>) -> Self {
        self.version = version;
        self
    }

    /// Selects the Rust library artifact name, `demo` for `libdemo.a`.
    pub fn native_library(mut self, library: impl Into<String>) -> Self {
        self.library = Some(library.into());
        self
    }

    /// Records the crate manifest path, relative to the extension directory,
    /// that the generated `extconf.rb` builds when no prebuilt library exists.
    pub fn cargo_manifest(mut self, manifest: impl Into<String>) -> Self {
        self.cargo_manifest = Some(manifest.into());
        self
    }

    /// Records the cargo feature selection of the binding expansion: the
    /// active features, comma separated, and the cargo arguments that
    /// selected them. The generated `extconf.rb` replays the arguments, so
    /// the library exports every function the extension calls.
    pub fn cargo_features(mut self, active: impl Into<String>, arguments: Vec<String>) -> Self {
        self.active_features = active.into();
        self.feature_args = arguments;
        self
    }

    /// Creates the backend target stack for this Ruby host.
    ///
    /// Ruby calls the C ABI directly, so the stack is `CBridge` alone. The
    /// bridge writes its header beside the extension source.
    /// The header directory uses the same stem as the Ruby package files.
    pub fn into_target(self, bindings: &Bindings<Native>) -> Result<Target<Self, CBridge>> {
        let header = format!("{}/boltffi.h", extension::directory(&self.stem(bindings)));
        Ok(Target::new(self, CBridge::new(header)?))
    }

    /// The gem name: the configured name, or the Cargo package name.
    fn gem(&self, bindings: &Bindings<Native>) -> String {
        self.gem
            .clone()
            .unwrap_or_else(|| default_gem(bindings.package().name()))
    }

    fn stem(&self, bindings: &Bindings<Native>) -> String {
        extension_stem(&self.gem(bindings))
    }

    fn module(&self, bindings: &Bindings<Native>) -> Result<ConstantPath> {
        self.module
            .clone()
            .map(Ok)
            .unwrap_or_else(|| default_module(bindings.package().name()))
    }
}

impl host::HostBackend for RubyCExtHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
    type Syntax = super::syntax::Syntax;

    fn name(&self) -> &'static str {
        "ruby"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .unsupported(
                BindingCapability::Records,
                "records are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Functions,
                "functions are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Enums,
                "enums are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Classes,
                "classes are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Callbacks,
                "callbacks are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Streams,
                "streams are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::Constants,
                "constants are not implemented in the Ruby host",
            )
            .unsupported(
                BindingCapability::CustomTypes,
                "custom types are not implemented in the Ruby host",
            )
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        _decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("record")
    }

    fn enumeration(
        &self,
        _decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("enum")
    }

    fn function(
        &self,
        _decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("function")
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("class")
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("callback")
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("stream")
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("constant")
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        unsupported("custom type")
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        let gem = self.gem(bindings);
        let stem = self.stem(bindings);
        let module = self.module(bindings)?;
        let artifact = self
            .library
            .clone()
            .unwrap_or_else(|| package_snake(bindings.package().name()));
        let version = self
            .version
            .clone()
            .or_else(|| bindings.package().version().map(str::to_owned))
            .unwrap_or_else(|| "0.1.0".to_owned());
        let crate_name = bindings.package().name().as_path_string();
        let package = Package {
            gem: &gem,
            version: &version,
            stem: &stem,
            module: &module,
            artifact: &artifact,
            cargo_manifest: self.cargo_manifest.as_deref(),
            active_features: &self.active_features,
            feature_args: &self.feature_args,
            crate_name: &crate_name,
        };
        let mut files = vec![
            extension::render(&stem, &module, declarations)?,
            runtime::header(&extension::directory(&stem))?,
        ];
        files.extend(package.render()?);
        Ok(GeneratedOutput::new(files, Vec::new()))
    }
}

impl sealed::HostBackend for RubyCExtHost {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{CanonicalName, NamePart, lower};

    use super::*;
    use crate::core::{CapabilityStatus, Error};

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", Some("1.2.3".to_owned())),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn render(host: RubyCExtHost, source: &str) -> GeneratedOutput {
        let bindings = bindings(source);
        host.into_target(&bindings)
            .expect("Ruby target")
            .render(&bindings)
            .expect("Ruby target should render")
    }

    fn file<'output>(output: &'output GeneratedOutput, path: &str) -> &'output str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new(path))
            .map(|file| file.contents())
            .unwrap_or_else(|| panic!("generated file {path}"))
    }

    fn paths(output: &GeneratedOutput) -> Vec<String> {
        output
            .files()
            .iter()
            .map(|file| file.path().as_path().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn multipart_package_names_share_one_extension_stem() {
        let mut serialized = serde_json::to_value(bindings("")).unwrap();
        serialized["package"]["name"] = serde_json::to_value(CanonicalName::new(vec![
            NamePart::new("my"),
            NamePart::new("lib"),
        ]))
        .unwrap();
        let bindings: Bindings<Native> = serde_json::from_value(serialized).unwrap();
        let output = RubyCExtHost::new()
            .into_target(&bindings)
            .unwrap()
            .render(&bindings)
            .unwrap();
        assert_eq!(
            paths(&output),
            [
                "ext/my_lib/boltffi.h",
                "ext/my_lib/my_lib.c",
                "ext/my_lib/boltffi_ruby.h",
                "lib/my_lib.rb",
                "ext/my_lib/extconf.rb",
                "my_lib.gemspec",
            ]
        );
        assert!(file(&output, "ext/my_lib/my_lib.c").contains("void Init_my_lib(void)"));
    }

    #[test]
    fn complete_rendering_rejects_unsupported_functions() {
        let bindings = bindings("#[export] pub fn echo(value: u32) -> u32 { value }");
        let target = RubyCExtHost::new().into_target(&bindings).unwrap();
        assert!(matches!(
            target.render(&bindings),
            Err(Error::BindingCapability {
                target: "ruby",
                capability: BindingCapability::Functions,
                status: CapabilityStatus::Unsupported { .. },
            })
        ));
    }

    #[test]
    fn partial_rendering_reports_each_unsupported_declaration() {
        let bindings = bindings(
            r#"
            #[data]
            pub struct Point { pub x: i32, pub y: i32 }
            #[export]
            pub fn echo(value: u32) -> u32 { value }
            "#,
        );
        let output = RubyCExtHost::new()
            .into_target(&bindings)
            .unwrap()
            .render_partial(&bindings)
            .unwrap();
        assert!(!output.coverage().is_complete());
        let mut unsupported: Vec<_> = output
            .coverage()
            .unsupported()
            .iter()
            .map(|entry| (entry.declaration().kind(), entry.declaration().name()))
            .collect();
        unsupported.sort_unstable();
        assert_eq!(unsupported, [("function", "echo"), ("record", "point")]);
        assert!(!file(&output, "ext/demo/demo.c").contains("echo"));
    }

    #[test]
    fn ruby_package_files_use_the_configured_gem_module_and_manifest() {
        let host = RubyCExtHost::new()
            .gem_name("my-lib")
            .module_name("MyLib::Native")
            .expect("valid module")
            .native_library("my_lib")
            .cargo_manifest("../../Cargo.toml")
            .cargo_features(
                "default,ffi",
                vec!["--features".to_owned(), "my-lib/ffi".to_owned()],
            );
        let output = render(host, "");

        assert_eq!(
            paths(&output),
            [
                "ext/my_lib/boltffi.h",
                "ext/my_lib/my_lib.c",
                "ext/my_lib/boltffi_ruby.h",
                "lib/my_lib.rb",
                "ext/my_lib/extconf.rb",
                "my-lib.gemspec",
            ]
        );
        insta::assert_snapshot!(
            "ruby_package_files",
            ["lib/my_lib.rb", "ext/my_lib/extconf.rb", "my-lib.gemspec",]
                .map(|path| format!("==> {path}\n{}", file(&output, path)))
                .join("\n")
        );
        assert!(file(&output, "ext/my_lib/my_lib.c").contains(
            "RUBY_FUNC_EXPORTED void Init_my_lib(void) {\n    VALUE boltffi_module = rb_define_module(\"MyLib\");\n    boltffi_module = rb_define_module_under(boltffi_module, \"Native\");"
        ));
    }
}

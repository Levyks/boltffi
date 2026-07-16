//! Dart target rendered through the C ABI bridge.

mod call;
mod codec;
mod ffi;
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
    bridge::c::{CBridge, CBridgeContract},
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error, FilePath,
        GeneratedFile, GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration,
        Result, Target, contract::sealed, host,
    },
};

use syntax::Syntax;

/// Dart host renderer configuration.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct DartHost {
    package: String,
    artifact: String,
    library_file: PathBuf,
    c_header: PathBuf,
}

impl DartHost {
    const TARGET: &'static str = "dart";

    /// Creates a Dart package renderer.
    pub fn new(package: impl Into<String>, artifact: impl Into<String>) -> Result<Self> {
        let package = package.into();
        let artifact = artifact.into();
        if package.is_empty() || artifact.is_empty() {
            return Err(Error::UnsupportedTarget {
                target: Self::TARGET,
                shape: "empty Dart package or artifact name",
            });
        }
        Ok(Self {
            library_file: PathBuf::from("lib").join(format!("{package}.dart")),
            c_header: PathBuf::from("native").join("boltffi.h"),
            package,
            artifact,
        })
    }

    /// Creates the metadata-backed Dart target.
    pub fn into_target(self) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(
            self.clone(),
            CBridge::new(self.c_header.clone())?,
        ))
    }

    fn unsupported<T>(&self, shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: Self::TARGET,
            shape,
        })
    }
}

impl host::HostBackend for DartHost {
    type Surface = Native;
    type Bridge = CBridgeContract;
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
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn record(
        &self,
        decl: &RecordDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::record(decl, bridge, context)
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::enumeration(decl, bridge, context)
    }

    fn function(
        &self,
        decl: &FunctionDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::function(decl, bridge, context)
    }

    fn class(
        &self,
        decl: &ClassDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::class(decl, bridge, context)
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::callback(decl, bridge, context)
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Native>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        self.unsupported("Dart stream rendering")
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Native>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        self.unsupported("Dart constant rendering")
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        self.unsupported("Dart custom type rendering")
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Native>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Native>,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Result<GeneratedOutput> {
        let body = declarations
            .into_iter()
            .map(RenderedDeclaration::into_parts)
            .map(|(_, emitted)| emitted.primary_chunk().as_str().to_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let runtime = include_str!("runtime.dart.txt");
        let ffi = ffi::render(_bridge)?;
        let library = format!("{runtime}\n\n{body}\n{ffi}");
        let pubspec = format!(
            "name: {}\n\nenvironment:\n  sdk: ^3.10.8\n\nresolution: workspace\n\ndependencies:\n  path: ^1.9.0\n  ffi: ^2.2.0\n  hooks: ^1.0.2\n  logging: ^1.3.0\n  code_assets: ^1.0.0\n  meta: ^1.17.0\n",
            self.package
        );
        let hook =
            include_str!("hook.build.dart.txt").replace("{{ artifact_name }}", &self.artifact);
        Ok(GeneratedOutput::new(
            vec![
                GeneratedFile::new(FilePath::new(&self.library_file)?, library),
                GeneratedFile::new(FilePath::new("pubspec.yaml")?, pubspec),
                GeneratedFile::new(FilePath::new("hook/build.dart")?, hook),
                GeneratedFile::new(
                    FilePath::new("dart_target.json")?,
                    format!("{{\"artifact\":\"{}\"}}\n", self.artifact),
                ),
            ],
            Vec::new(),
        ))
    }
}

impl sealed::HostBackend for DartHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Native, lower};
    use boltffi_scan::scan_file;

    use super::DartHost;

    #[test]
    fn creates_metadata_backed_target() {
        DartHost::new("demo", "demo")
            .expect("host")
            .into_target()
            .expect("target");
    }

    #[test]
    fn renders_records_and_enums_from_metadata() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[data]
                pub struct Person { pub name: String, pub age: u32 }

                #[data]
                #[repr(C)]
                pub struct Point { pub x: i32, pub y: i32 }

                #[data]
                pub enum State { Idle, Named(String) }

                #[export]
                pub fn translate(point: Point) -> Point { point }
                "#,
            )
            .expect("source"),
            PackageInfo::new("demo", None),
        )
        .expect("scan");
        let bindings = lower::<Native>(&source).expect("lower");
        let output = DartHost::new("demo", "demo")
            .expect("host")
            .into_target()
            .expect("target")
            .render_partial(&bindings)
            .expect("render");
        let library = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.dart"))
            .expect("library")
            .contents();
        assert!(library.contains("final class Person"));
        assert!(library.contains("_toStruct()"));
        assert!(library.contains("Point._fromStruct"));
        assert!(library.contains("Point translate(Point point)"));
        assert!(library.contains("sealed class State"));
        assert!(library.contains("State$Named"));
    }

    #[test]
    fn renders_opaque_runner_with_dart_callback() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub trait Operation: Send + Sync {
                    fn apply(&self, value: i32) -> i32;
                    fn finish(&self, value: i32) -> i32;
                }
                pub struct Runner;
                #[export]
                impl Runner {
                    pub fn new(operation: Box<dyn Operation>) -> Self { unimplemented!() }
                    pub fn run(&self, value: i32, iterations: u32) -> i32 { unimplemented!() }
                }
                "#,
            )
            .expect("source"),
            PackageInfo::new("demo", None),
        )
        .expect("scan");
        let bindings = lower::<Native>(&source).expect("lower");
        let output = DartHost::new("demo", "demo")
            .expect("host")
            .into_target()
            .expect("target")
            .render_partial(&bindings)
            .expect("render");
        let library = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.dart"))
            .expect("library")
            .contents();
        assert!(library.contains("abstract interface class Operation"));
        assert!(library.contains("final class Runner"));
        assert!(library.contains("_I$OperationHandleMap.createHandle(operation)"));
        assert!(library.contains("return impl.apply(valueDecoded);"));
    }

    #[test]
    fn renders_async_callback_and_async_class_method() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub trait Operation: Send + Sync {
                    fn apply(&self, value: i32) -> i32;
                    async fn finish(&self, value: i32) -> Result<i32, String>;
                }
                pub struct Runner;
                #[export]
                impl Runner {
                    pub fn new(operation: Box<dyn Operation>) -> Self { unimplemented!() }
                    pub async fn run(&self, value: i32) -> Result<i32, String> { unimplemented!() }
                }
                "#,
            )
            .expect("source"),
            PackageInfo::new("demo", None),
        )
        .expect("scan");
        let bindings = lower::<Native>(&source).expect("lower");
        let output = DartHost::new("demo", "demo")
            .expect("host")
            .into_target()
            .expect("target")
            .render_partial(&bindings)
            .expect("render");
        let library = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.dart"))
            .expect("library")
            .contents();
        assert!(library.contains("Future<BoltFFIResult<int, String>> finish(int value)"));
        assert!(library.contains("final buffer = writer.toRustBuffer();"));
        assert!(library.contains("case BoltFFIResult$Ok(:final value):"));
        assert!(library.contains("completionCode = 100;"));
        assert!(library.contains("Future<int> run(int value)"));
        assert!(library.contains("_$$BoltFFIAsync.create<int>"));
    }

    #[test]
    fn renders_sync_encoded_builtin_roundtrips() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn echo_duration(value: std::time::Duration) -> std::time::Duration {
                    value
                }
                #[export]
                pub fn echo_system_time(value: std::time::SystemTime) -> std::time::SystemTime {
                    value
                }
                #[export]
                pub fn echo_uuid(value: uuid::Uuid) -> uuid::Uuid { value }
                #[export]
                pub fn echo_url(value: url::Url) -> url::Url { value }
                "#,
            )
            .expect("source"),
            PackageInfo::new("demo", None),
        )
        .expect("scan");
        let bindings = lower::<Native>(&source).expect("lower");
        let output = DartHost::new("demo", "demo")
            .expect("host")
            .into_target()
            .expect("target")
            .render_partial(&bindings)
            .expect("render");
        let library = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("demo.dart"))
            .expect("library")
            .contents();

        assert!(library.contains("Duration echoDuration(Duration value)"));
        assert!(library.contains("DateTime echoSystemTime(DateTime value)"));
        assert!(library.contains("String echoUuid(String value)"));
        assert!(library.contains("String echoUrl(String value)"));
        assert!(library.contains("final reader = _$$WireReader(buffer.ptr, buffer.len);"));
        assert!(library.contains("_f$boltffi_free_buf(buffer);"));
    }
}

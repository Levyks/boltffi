//! Dart target rendered through the C ABI bridge.

mod call;
mod closure;
mod codec;
mod ffi;
mod foreign;
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
    custom_mappings: crate::core::CustomTypeMappingSet,
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
            custom_mappings: crate::core::CustomTypeMappingSet::default(),
        })
    }

    /// Registers a Dart API mapping for a custom type.
    pub fn custom_type_mapping(
        mut self,
        custom_type: impl Into<String>,
        mapping: crate::CustomTypeMapping,
    ) -> Self {
        self.custom_mappings.insert(custom_type, mapping);
        self
    }

    /// Creates the metadata-backed Dart target.
    pub fn into_target(self) -> Result<Target<Self, CBridge>> {
        Ok(Target::new(
            self.clone(),
            CBridge::new(self.c_header.clone())?,
        ))
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
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
            .stable(BindingCapability::InternedString)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::CAbi)
    }

    fn custom_type_mappings(
        &self,
        bindings: &Bindings<Native>,
    ) -> Result<crate::core::ResolvedCustomTypeMappings> {
        self.custom_mappings
            .resolve(bindings, Self::TARGET, |declaration| {
                name_style::upper_camel(declaration.name())
            })
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
        decl: &StreamDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::stream(decl, bridge, context)
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Native>,
        bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::constant(decl, bridge, context)
    }

    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        context: &RenderContext<Native>,
    ) -> Result<Emitted> {
        render::custom_type(decl, context)
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Native>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Native>,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Result<GeneratedOutput> {
        use std::collections::BTreeMap;

        use crate::core::{AuxChunk, HelperId, TextChunk};

        let mut helpers = BTreeMap::<HelperId, TextChunk>::new();
        let mut body = String::new();
        for declaration in declarations {
            let (_, emitted) = declaration.into_parts();
            for aux in emitted.aux_chunks() {
                if let AuxChunk::Helper { id, text } = aux {
                    helpers.entry(id.clone()).or_insert_with(|| text.clone());
                }
            }
            body.push_str(emitted.primary_chunk().as_str());
        }
        let helpers = helpers
            .into_values()
            .map(|chunk| chunk.as_str().to_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let runtime = include_str!("runtime.dart.txt");
        let ffi = ffi::render(_bridge)?;
        let library = format!("{runtime}\n\n{helpers}{body}\n{ffi}");
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
    fn renders_static_class_methods_without_receiver() {
        let source = scan_file(
            syn::parse_str(
                r#"
                pub struct MathUtils;
                #[export]
                impl MathUtils {
                    pub fn add(a: i32, b: i32) -> i32 { a + b }
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
        assert!(library.contains("int add(int a, int b)"));
        assert!(
            !library.contains("add(_rawHandle"),
            "static methods must not pass a receiver handle"
        );
    }

    #[test]
    fn renders_inline_closures() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn apply_closure(f: impl Fn(i32) -> i32, value: i32) -> i32 { f(value) }
                #[export]
                pub fn apply_string_closure(f: impl Fn(String) -> String, s: String) -> String { f(s) }
                #[export]
                pub fn apply_void_closure(f: impl Fn(i32), value: i32) { f(value) }
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

        assert!(library.contains("int applyClosure(int Function(int) f, int value)"));
        assert!(library.contains("String applyStringClosure(String Function(String) f, String s)"));
        assert!(library.contains("void applyVoidClosure(void Function(int) f, int value)"));
        assert!(library.contains("final class _Cl$"));
        assert!(library.contains(".callPtr"));
        assert!(library.contains(".releasePtr"));
        assert!(library.contains("Pointer<$$ffi.Void>.fromAddress"));
    }

    #[test]
    fn renders_async_buffer_return_slots() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub async fn async_find(values: Vec<i32>) -> Option<i32> { unimplemented!() }
                #[export]
                pub async fn async_double(values: Vec<i32>) -> Vec<i32> { unimplemented!() }
                #[export]
                pub async fn async_echo(message: String) -> String { unimplemented!() }
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

        assert!(library.contains("Future<int?> asyncFind(List<int> values)"));
        assert!(library.contains("Future<List<int>> asyncDouble(List<int> values)"));
        assert!(library.contains("Future<String> asyncEcho(String message)"));
        assert!(library.contains("final result = _f$"));
        assert!(library.contains("reader.readOptional((reader) => reader.readI32())"));
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

        let hook = output
            .files()
            .iter()
            .find(|file| file.path().as_path().ends_with("hook/build.dart"))
            .expect("build hook")
            .contents();
        assert!(hook.contains("(OS.windows, Architecture.x64) => 'x86_64-pc-windows-msvc'"));
    }

    #[test]
    fn renders_constants_and_fallible_sync_calls() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[data]
                #[repr(i32)]
                pub enum Mode { Fast = 1, Safe = 2 }

                #[error]
                pub struct ParseError { pub message: String }

                #[export]
                pub const ANSWER: u32 = 42;
                #[export]
                pub const DEFAULT_MODE: Mode = Mode::Safe;
                #[export]
                pub const GREETING: &str = "hello $dart";

                #[export]
                pub fn parse(value: String) -> Result<u32, ParseError> { unimplemented!() }
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

        assert!(library.contains("const int answer = 42;"));
        assert!(library.contains("const Mode defaultMode = Mode.safe;"));
        assert!(library.contains("const String greeting = 'hello \\$dart';"));
        assert!(library.contains("int parse(String value)"));
        assert!(library.contains("final success = $$extffi.calloc<"));
        assert!(library.contains("throw ParseError._decode(errorReader);"));
    }

    #[test]
    fn renders_scalar_options() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[export]
                pub fn echo_optional(value: Option<u32>) -> Option<u32> { value }
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

        assert!(library.contains("int? echoOptional(int? value)"));
        assert!(
            library.contains("writeOptional(value, (value, writer) => writer.writeU32(value))")
        );
        assert!(library.contains("return reader.readOptional((reader) => reader.readU32());"));
    }

    #[test]
    fn renders_direct_vectors() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[data]
                #[repr(C)]
                pub struct Point { pub x: i32, pub y: i32 }

                #[export]
                pub fn echo_numbers(values: Vec<i32>) -> Vec<i32> { values }
                #[export]
                pub fn mutate_numbers(values: &mut [i32]) { values.reverse(); }
                #[export]
                pub fn echo_points(values: Vec<Point>) -> Vec<Point> { values }
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

        assert!(library.contains("List<int> echoNumbers(List<int> values)"));
        assert!(library.contains("calloc<$$ffi.Int32>(values.length)"));
        assert!(library.contains("values[i] = (_$vectorValues + i).value"));
        assert!(library.contains("values.length * $$ffi.sizeOf<_C$"));
        assert!(library.contains("Point._fromStruct((raw + i).ref)"));
    }

    #[test]
    fn renders_all_stream_modes() {
        let source = scan_file(
            syn::parse_str(
                r#"
                use std::sync::Arc;
                use boltffi::EventSubscription;
                #[data]
                pub struct Message { pub text: String }
                pub struct EventBus;
                #[export]
                impl EventBus {
                    pub fn new() -> Self { EventBus }
                    #[ffi_stream(item = i32)]
                    pub fn values(&self) -> Arc<EventSubscription<i32>> { unimplemented!() }
                    #[ffi_stream(item = Message, mode = "batch")]
                    pub fn messages(&self) -> Arc<EventSubscription<Message>> { unimplemented!() }
                    #[ffi_stream(item = i32, mode = "callback")]
                    pub fn callbacks(&self) -> Arc<EventSubscription<i32>> { unimplemented!() }
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

        assert!(library.contains("$$async.Stream<int> values()"));
        assert!(library.contains("final class MessagesSubscription"));
        assert!(
            library.contains("BoltFFIStreamCancellation callbacks(void Function(int) callback)")
        );
        assert!(library.contains("_$$BoltFFIStreamPump<int>"));
        assert!(library.contains("final count = reader.readU32();"));
    }

    #[test]
    fn renders_rich_synchronous_callbacks() {
        let source = scan_file(
            syn::parse_str(
                r#"
                #[data]
                #[repr(C)]
                pub struct Point { pub x: i32, pub y: i32 }
                #[error]
                pub struct CallbackError { pub message: String }
                #[export]
                pub trait RichCallback: Send + Sync {
                    fn optional(&self, value: Option<u32>) -> Option<u32>;
                    fn numbers(&self, values: Vec<i32>) -> Vec<i32>;
                    fn points(&self, values: Vec<Point>) -> Vec<Point>;
                    fn checked(&self, value: i32) -> Result<u32, CallbackError>;
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

        assert!(library.contains("int? optional(int? value)"));
        assert!(library.contains("List<int> numbers(List<int> values)"));
        assert!(library.contains("BoltFFIResult<int, CallbackError> checked(int value)"));
        assert!(library.contains("case BoltFFIResult$Err(:final value):"));
        assert!(library.contains("return _$$emptyBuf();"));
        // Native callback scalar-option args use an empty buffer for None.
        assert!(library.contains(
            "value_len == 0 ? null : _$$WireReader(value_ptr, value_len).readOptional((reader) => reader.readU32())"
        ));
    }
}

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use boltffi_ast::PackageInfo;
use boltffi_backend::{
    CoverageMode, Error,
    target::java::{JavaDesktopLoader, JavaHost, JavaVersion},
};
use boltffi_binding::{DeclarationRef, Native, lower};

mod java_toolchain;

use java_toolchain::{JavaCompiler, JavaEightCompilation};

const PRIMITIVE_FUNCTIONS: &str = r#"
    #[export]
    pub fn carriers(
        flag: bool,
        signed_byte: i8,
        unsigned_byte: u8,
        signed_short: i16,
        unsigned_short: u16,
        signed_word: i32,
        unsigned_word: u32,
        signed_wide: i64,
        unsigned_wide: u64,
        signed_size: isize,
        unsigned_size: usize,
        single: f32,
        double: f64,
    ) -> u64 {
        unsigned_wide
    }

    #[export]
    pub fn enabled(flag: bool) -> bool {
        flag
    }

    #[export]
    pub fn refresh() {}
"#;

const DIRECT_RECORD_FUNCTIONS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct Point {
        pub x: f64,
        pub y: f64,
        pub visible: bool,
        pub color: u32,
    }

    #[export]
    pub fn echo_point(point: Point) -> Point { point }
"#;

const DIRECT_RECORD_CALLS: &str = r#"
    #[repr(C)]
    #[data]
    pub struct Counter {
        pub value: i32,
    }

    #[data(impl)]
    impl Counter {
        pub fn new(value: i32) -> Self { Self { value } }
        pub fn zero() -> Self { Self { value: 0 } }
        pub fn current(&self) -> i32 { self.value }
        pub fn increment(&mut self, amount: i32) { self.value += amount; }
        pub fn added(&self, other: Counter) -> Counter {
            Counter { value: self.value + other.value }
        }
    }
"#;

const ENCODED_RECORD: &str = r#"
    #[data]
    pub struct Profile {
        pub name: String,
        pub samples: Vec<i32>,
        pub marker: Option<i32>,
    }
"#;

const RESULT_RECORD: &str = r#"
    #[data]
    pub struct ResultHolder {
        pub outcome: Result<Vec<i32>, Option<String>>,
    }
"#;

const ENCODED_RECORD_CALLS: &str = r#"
    #[data]
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    #[data(impl)]
    impl Point {
        pub fn new(x: f64, y: f64) -> Self { Self { x, y } }
        pub fn origin() -> Self { Self { x: 0.0, y: 0.0 } }
        pub fn try_unit(x: f64, y: f64) -> Result<Self, String> {
            let length = (x * x + y * y).sqrt();
            if length == 0.0 {
                Err("cannot normalize zero vector".to_owned())
            } else {
                Ok(Self { x: x / length, y: y / length })
            }
        }
        pub fn checked_unit(x: f64, y: f64) -> Option<Self> {
            let length = (x * x + y * y).sqrt();
            (length != 0.0).then(|| Self { x: x / length, y: y / length })
        }
        pub fn distance(&self) -> f64 { (self.x * self.x + self.y * self.y).sqrt() }
        pub fn scale(&mut self, factor: f64) {
            self.x *= factor;
            self.y *= factor;
        }
        pub fn add(&self, other: Point) -> Point {
            Point { x: self.x + other.x, y: self.y + other.y }
        }
        pub fn path_length(points: Vec<Point>) -> f64 {
            points.windows(2).map(|pair| {
                let dx = pair[1].x - pair[0].x;
                let dy = pair[1].y - pair[0].y;
                (dx * dx + dy * dy).sqrt()
            }).sum()
        }
    }

    #[export]
    pub fn echo_point(point: Point) -> Point { point }
"#;

const RECORD_DEFAULTS: &str = r#"
    #[data]
    pub struct ServiceConfig {
        pub name: String,
        #[boltffi::default(3)]
        pub retries: i32,
        #[boltffi::default("standard")]
        pub region: String,
        /// Optional endpoint.
        #[boltffi::default(None)]
        pub endpoint: Option<String>,
        #[boltffi::default("https://default")]
        pub backup_endpoint: Option<String>,
    }
"#;

const ERROR_RECORD: &str = r#"
    #[error]
    pub struct AppError {
        pub code: i32,
        pub message: String,
    }

    #[export]
    pub fn may_fail(valid: bool) -> Result<String, AppError> {
        match valid {
            true => Ok("ok".to_owned()),
            false => Err(AppError { code: 400, message: "invalid".to_owned() }),
        }
    }
"#;

const ENUMS: &str = r#"
    #[repr(u8)]
    #[data]
    pub enum Mode {
        Fast = 1,
        Slow = 7,
    }

    #[repr(u64)]
    #[data]
    pub enum WideMode {
        Zero = 0,
        Maximum = 18446744073709551615,
    }

    #[data(impl)]
    impl Mode {
        pub fn fast() -> Self { Self::Fast }
        pub fn is_fast(&self) -> bool { matches!(self, Self::Fast) }
    }

    #[data]
    pub enum Shape {
        Empty,
        Circle { radius: f64 },
        Label(String),
    }

    #[data(impl)]
    impl Shape {
        pub fn empty() -> Self { Self::Empty }
        pub fn is_empty(&self) -> bool { matches!(self, Self::Empty) }
    }

    #[export]
    pub fn echo_mode(value: Mode) -> Mode { value }

    #[export]
    pub fn echo_modes(values: Vec<Mode>) -> Vec<Mode> { values }

    #[export]
    pub fn echo_wide_mode(value: WideMode) -> WideMode { value }

    #[export]
    pub fn echo_shape(value: Shape) -> Shape { value }
"#;

const ERROR_ENUMS: &str = r#"
    #[error]
    pub enum ParseError {
        Missing,
        Invalid,
    }

    #[error]
    pub enum ApiError {
        Message { message: String },
        Code(i32),
    }

    #[export]
    pub fn parse(valid: bool) -> Result<i32, ParseError> {
        if valid { Ok(1) } else { Err(ParseError::Invalid) }
    }

    #[export]
    pub fn request(valid: bool) -> Result<String, ApiError> {
        if valid {
            Ok("ok".to_owned())
        } else {
            Err(ApiError::Message { message: "failed".to_owned() })
        }
    }
"#;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid Java source fixture");
    let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
        .expect("Java fixture should scan");
    lower::<Native>(&source).expect("Java fixture should lower")
}

fn host() -> JavaHost {
    JavaHost::new("com.boltffi.demo", "Demo")
        .expect("Java host")
        .desktop_loader(JavaDesktopLoader::None)
}

fn render(source: &str, coverage: CoverageMode) -> boltffi_backend::GeneratedOutput {
    render_with_host(source, coverage, host())
}

fn render_with_host(
    source: &str,
    coverage: CoverageMode,
    host: JavaHost,
) -> boltffi_backend::GeneratedOutput {
    let bindings = bindings(source);
    host.render_with_coverage(&bindings, coverage)
        .expect("Java target should render")
}

fn validate_host(host: JavaHost) -> Result<boltffi_backend::GeneratedOutput, Error> {
    host.render_with_coverage(&bindings(""), CoverageMode::Complete)
}

fn java_source<'output>(
    output: &'output boltffi_backend::GeneratedOutput,
    package: &str,
    file: &str,
) -> &'output str {
    let path = PathBuf::from(package.replace('.', "/")).join(format!("{file}.java"));
    output
        .files()
        .iter()
        .find(|generated| generated.path().as_path() == path)
        .expect("Java target should emit source")
        .contents()
}

#[test]
fn java_target_renders_primitive_function_stack() {
    insta::assert_snapshot!(java_source(
        &render(PRIMITIVE_FUNCTIONS, CoverageMode::Complete),
        "com.boltffi.demo",
        "Demo"
    ));
}

#[test]
fn java_target_renders_direct_record_stack() {
    let output = render(DIRECT_RECORD_FUNCTIONS, CoverageMode::Complete);
    insta::assert_snapshot!(
        "java_direct_record_stack",
        format!(
            "===== Demo.java =====\n{}\n===== Point.java =====\n{}",
            java_source(&output, "com.boltffi.demo", "Demo"),
            java_source(&output, "com.boltffi.demo", "Point"),
        )
    );
}

#[test]
fn java_target_renders_direct_record_associated_calls() {
    let output = render(DIRECT_RECORD_CALLS, CoverageMode::Complete);
    let counter = java_source(&output, "com.boltffi.demo", "Counter");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(counter.contains("public static Counter _new(int value)"));
    assert!(counter.contains("public static Counter zero()"));
    assert!(counter.contains("public int current()"));
    assert!(counter.contains("public Counter increment(int amount)"));
    assert!(counter.contains("return Counter.fromDirectBuffer(__boltffi_receiver);"));
    assert!(counter.contains("public Counter added(Counter other)"));
    assert!(counter.contains("other.toDirectBuffer()"));
    assert!(module.contains("static native byte[]"));
    assert!(module.contains("static native int"));
    assert!(module.contains("static native void"));
}

#[test]
fn java_target_renders_encoded_record_fields_through_codec_plans() {
    let output = render(ENCODED_RECORD, CoverageMode::Complete);
    let profile = java_source(&output, "com.boltffi.demo", "Profile");

    assert!(profile.contains("public final class Profile"));
    assert!(profile.contains("String name"));
    assert!(profile.contains("int[] samples"));
    assert!(profile.contains("java.util.Optional<Integer> marker"));
    assert!(profile.contains("reader.readString()"));
    assert!(profile.contains("reader.readIntArray()"));
    assert!(profile.contains("reader.readOptional"));
    assert!(profile.contains("writer.writeString"));
    assert!(profile.contains("writer.writeIntArray"));
    assert!(profile.contains("writer.writeOptional"));
}

#[test]
fn java_target_emits_one_public_result_type_for_result_records() {
    let output = render(RESULT_RECORD, CoverageMode::Complete);
    let holder = java_source(&output, "com.boltffi.demo", "ResultHolder");
    let result = java_source(&output, "com.boltffi.demo", "BoltFFIResult");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(holder.contains("BoltFFIResult<int[], java.util.Optional<String>> outcome"));
    assert!(result.contains("public final class BoltFFIResult<Ok, Err>"));
    assert!(result.contains("public static <Ok, Err> BoltFFIResult<Ok, Err> ok"));
    assert!(result.contains("public boolean isOk()"));
    assert!(result.contains("public String toString()"));
    assert!(!module.contains("class BoltFfiResult"));
    assert_eq!(
        output
            .files()
            .iter()
            .filter(|file| {
                file.path().as_path() == Path::new("com/boltffi/demo/BoltFFIResult.java")
            })
            .count(),
        1
    );
}

#[test]
fn java_target_rejects_public_result_file_collisions() {
    let bindings = bindings(RESULT_RECORD);
    let error = JavaHost::new("com.boltffi.demo", "BoltFFIResult")
        .expect("Java result collision host")
        .desktop_loader(JavaDesktopLoader::None)
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("public result file should not replace the module file");

    assert_eq!(
        error,
        Error::JavaNameCollision {
            scope: "com.boltffi.demo".to_owned(),
            name: "BoltFFIResult".to_owned(),
        }
    );
}

#[test]
fn java_target_renders_trailing_record_default_constructors() {
    let output = render(RECORD_DEFAULTS, CoverageMode::Complete);
    let config = java_source(&output, "com.boltffi.demo", "ServiceConfig");

    assert!(config.contains("public ServiceConfig(String name)"));
    assert!(config.contains(
        "this(name, 3, \"standard\", java.util.Optional.empty(), java.util.Optional.of(\"https://default\"));"
    ));
    assert!(config.contains("public ServiceConfig(String name, int retries)"));
    assert!(config.contains("* Optional endpoint."));
}

#[test]
fn java_target_uses_class_semantics_for_primitive_array_fields() {
    let output = render_with_host(
        ENCODED_RECORD,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_17)
            .expect("Java 17 host")
            .desktop_loader(JavaDesktopLoader::None),
    );
    let profile = java_source(&output, "com.boltffi.demo", "Profile");

    assert!(profile.contains("public final class Profile"));
    assert!(!profile.contains("public record Profile"));
    assert!(profile.contains("java.util.Arrays.equals(this.samples, other.samples)"));
    assert!(profile.contains("java.util.Arrays.hashCode(this.samples)"));
}

#[test]
fn java_target_uses_error_record_messages_for_exceptions() {
    let output = render(ERROR_RECORD, CoverageMode::Complete);
    let error = java_source(&output, "com.boltffi.demo", "AppError");

    assert!(error.contains("public final class AppError extends RuntimeException"));
    assert!(error.contains("super(message);"));
    assert!(error.contains("public AppError getError()"));
}

#[test]
fn java_target_renders_c_style_and_data_enums_from_binding_ir() {
    let output = render(ENUMS, CoverageMode::Complete);
    let mode = java_source(&output, "com.boltffi.demo", "Mode");
    let shape = java_source(&output, "com.boltffi.demo", "Shape");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(mode.contains("public enum Mode"));
    assert!(mode.contains("FAST((byte) (1))"));
    assert!(mode.contains("case (byte) (7): return SLOW;"));
    assert!(module.contains("writeByte"));
    assert!(module.contains("readByte()"));
    assert!(mode.contains("public boolean isFast()"));
    let wide = java_source(&output, "com.boltffi.demo", "WideMode");
    assert!(wide.contains("MAXIMUM(0xFFFFFFFFFFFFFFFFL)"));
    assert!(wide.contains("if (value == 0xFFFFFFFFFFFFFFFFL) return MAXIMUM;"));
    assert!(shape.contains("public abstract class Shape"));
    assert!(shape.contains("public static final class Empty extends Shape"));
    assert!(shape.contains("public static final class Circle extends Shape"));
    assert!(shape.contains("writer.writeDouble(this.radius);"));
    assert!(shape.contains("reader.readString()"));
    assert!(shape.contains("public boolean isEmpty()"));
    assert!(module.contains("static native byte"));
    assert!(module.contains("static native byte[]"));
}

#[test]
fn java_target_preserves_typed_enum_errors() {
    let output = render(ERROR_ENUMS, CoverageMode::Complete);
    let parse_error = java_source(&output, "com.boltffi.demo", "ParseError");
    let api_error = java_source(&output, "com.boltffi.demo", "ApiError");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(parse_error.contains("public enum ParseError"));
    assert!(parse_error.contains("public static final class Exception extends RuntimeException"));
    assert!(api_error.contains("public abstract class ApiError extends RuntimeException"));
    assert!(api_error.contains("super(message);"));
    assert!(module.contains("throw new ParseError.Exception"));
    assert!(module.contains("throw ApiError.fromReader"));
}

#[test]
fn java_seventeen_uses_sealed_data_enums_when_value_semantics_are_safe() {
    let output = render_with_host(
        ENUMS,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_17)
            .expect("Java 17 enum host"),
    );
    let shape = java_source(&output, "com.boltffi.demo", "Shape");

    assert!(shape.contains("public sealed interface Shape permits"));
    assert!(shape.contains("record Circle(double radius) implements Shape"));
    assert!(shape.contains("default boolean isEmpty()"));
}

#[test]
fn java_target_renders_encoded_record_calls_through_shared_jni_carriers() {
    let output = render(ENCODED_RECORD_CALLS, CoverageMode::Complete);
    let point = java_source(&output, "com.boltffi.demo", "Point");
    let module = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(point.contains("public static Point _new(double x, double y)"));
    assert!(point.contains("public static Point origin()"));
    assert!(point.contains("public static Point tryUnit(double x, double y)"));
    assert!(point.contains("catch (BoltFfiErrorBufferException __boltffi_error)"));
    assert!(point.contains("public static java.util.Optional<Point> checkedUnit"));
    assert!(point.contains("public double distance()"));
    assert!(point.contains("public Point scale(double factor)"));
    assert!(point.contains("public Point add(Point other)"));
    assert!(point.contains("public static double pathLength(java.util.List<Point> points)"));
    assert!(module.contains("java.nio.ByteBuffer point"));
    assert!(module.contains("int __boltffi_point_len"));
}

#[test]
fn java_partial_target_reports_backend_coverage() {
    let source = r#"
        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }

        #[export]
        pub fn increment(values: &mut [u64]) {
            values.iter_mut().for_each(|value| *value += 1);
        }
    "#;
    let bindings = bindings(source);
    let symbol = |name| {
        bindings
            .decls()
            .iter()
            .find_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function)
                    if function.name().source_spelling() == Some(name) =>
                {
                    Some(function.symbol().name().as_str())
                }
                _ => None,
            })
            .expect("function source symbol")
    };
    let add_symbol = symbol("add");
    let increment_symbol = symbol("increment");
    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("partial Java target should retain supported bindings");
    let unsupported = output.coverage().unsupported();
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(java.contains(&format!(
        "static native int {add_symbol}(int left, int right)"
    )));
    assert!(!java.contains("increment"));
    assert_eq!(unsupported.len(), 1);
    assert_eq!(unsupported[0].declaration().kind(), "function");
    assert_eq!(unsupported[0].declaration().name(), "increment");
    assert_eq!(
        unsupported[0].reason(),
        "mutable encoded function parameter"
    );
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(add_symbol))
    );
    assert!(
        output
            .files()
            .iter()
            .all(|file| !file.contents().contains(increment_symbol))
    );
}

#[test]
fn java_partial_target_retains_dependency_closed_direct_records() {
    let source = r#"
        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: i32,
        }

        #[export]
        pub fn keep_point(point: Point) -> Point { point }

        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }
    "#;
    let bindings = bindings(source);
    let symbol = |name| {
        bindings
            .decls()
            .iter()
            .find_map(|declaration| match DeclarationRef::from(declaration) {
                DeclarationRef::Function(function)
                    if function.name().source_spelling() == Some(name) =>
                {
                    Some(function.symbol().name().as_str())
                }
                _ => None,
            })
            .expect("function source symbol")
    };
    let keep_point_symbol = symbol("keep_point");
    let add_symbol = symbol("add");
    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("dependency-closed Java target");
    let coverage = output.coverage().unsupported();
    let java = java_source(&output, "com.boltffi.demo", "Demo");
    let point = java_source(&output, "com.boltffi.demo", "Point");

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(java.contains("public static Point keepPoint(Point point)"));
    assert!(java.contains("point.toDirectBuffer()"));
    assert!(java.contains("Point.fromByteArray"));
    assert!(point.contains("public final class Point"));
    assert!(point.contains("static final int STRUCT_SIZE = 4;"));
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(add_symbol))
    );
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(keep_point_symbol))
    );
    assert!(coverage.is_empty());
}

#[test]
fn java_partial_target_rejects_unsupported_functions_before_building_the_bridge() {
    let source = r#"
        #[export]
        pub fn add(left: i32, right: i32) -> i32 { left + right }

        #[export]
        pub async fn make_counter() -> impl Fn(u32) -> u32 {
            |value| value + 1
        }
    "#;
    let bindings = bindings(source);
    let symbols = bindings
        .decls()
        .iter()
        .filter_map(|declaration| match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function) => Some((
                function
                    .name()
                    .source_spelling()
                    .expect("function source spelling"),
                function.symbol().name().as_str(),
            )),
            _ => None,
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let add_symbol = symbols["add"];
    let rejected_symbol = symbols["make_counter"];

    let error = host()
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("complete Java target should preserve the JNI rejection");
    assert_eq!(
        error,
        Error::UnsupportedBridge {
            bridge: "jni",
            shape: "closure return out-pointer on a native method",
        }
    );

    let output = host()
        .render_with_coverage(&bindings, CoverageMode::Partial)
        .expect("partial Java target should remove the JNI rejection");
    let coverage = output.coverage().unsupported();
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static int add(int left, int right)"));
    assert!(
        output
            .files()
            .iter()
            .any(|file| file.contents().contains(add_symbol))
    );
    assert!(
        output
            .files()
            .iter()
            .all(|file| !file.contents().contains(rejected_symbol))
    );
    assert_eq!(coverage.len(), 1);
    assert_eq!(coverage[0].declaration().kind(), "function");
    assert_eq!(coverage[0].declaration().name(), "make::counter");
    assert_eq!(coverage[0].reason(), "asynchronous function");
}

#[test]
fn java_target_renders_encoded_byte_functions_in_complete_coverage() {
    let output = render(
        r#"
        #[export]
        pub fn echo_bytes(value: Vec<u8>) -> Vec<u8> { value }
        "#,
        CoverageMode::Complete,
    );
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static byte[] echoBytes(byte[] value)"));
    assert!(java.contains("WireWriterPool.acquire((4 + value.length))"));
    assert!(java.contains("reader.readBytes()"));
    assert!(output.coverage().unsupported().is_empty());
}

#[test]
fn java_target_specializes_string_sequences_without_codec_callbacks() {
    let output = render(
        r#"
        #[export]
        pub fn echo_strings(value: Vec<String>) -> Vec<String> { value }
        "#,
        CoverageMode::Complete,
    );
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("WireSizes.stringSequence(value)"));
    assert!(java.contains("writer.writeStringSequence(value)"));
    assert!(java.contains("reader.readStringSequence()"));
    assert!(!java.contains("writer.writeSequence(value"));
}

#[test]
fn java_target_rejects_generated_name_collisions() {
    assert!(matches!(
        JavaHost::new("com.boltffi.demo", "Native"),
        Err(Error::JavaNameCollision { scope, name })
            if scope == "com.boltffi.demo" && name == "Native"
    ));
    assert!(matches!(
        JavaHost::new("com.boltffi.demo", "BoltFFINativeRuntime"),
        Err(Error::JavaNameCollision { scope, name })
            if scope == "com.boltffi.demo" && name == "BoltFFINativeRuntime"
    ));
    assert!(JavaHost::new("com.boltffi.demo", "boltffinativeruntime").is_err());
    assert!(JavaHost::new("com.boltffi.demo", "BoltffiNativeRuntime").is_err());
    let collision = validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_jni_library("NativeCore")
            .unwrap()
            .desktop_fallback_library("nativecore")
            .unwrap(),
    )
    .expect_err("case-insensitive native library paths must not collide");
    assert_eq!(
        collision,
        Error::JavaNameCollision {
            scope: "desktop native libraries".to_owned(),
            name: "nativecore".to_owned(),
        }
    );
    assert!(
        validate_host(
            JavaHost::new("com.boltffi.demo", "Demo")
                .unwrap()
                .desktop_fallback_library("NativeCore")
                .unwrap()
                .desktop_jni_library("nativecore")
                .unwrap(),
        )
        .is_err()
    );
    validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_jni_library("NativeCore")
            .unwrap()
            .desktop_fallback_library("NativeCore")
            .unwrap(),
    )
    .expect("one shared native library should remain valid");
    validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_jni_library("BOLTFFI")
            .unwrap()
            .desktop_fallback_library("BOLTFFI_JNI")
            .unwrap(),
    )
    .expect("final native library pair should validate after both setters");
    validate_host(
        JavaHost::new("com.boltffi.demo", "Demo")
            .unwrap()
            .desktop_fallback_library("BOLTFFI_JNI")
            .unwrap()
            .desktop_jni_library("BOLTFFI")
            .unwrap(),
    )
    .expect("final native library pair should be independent of setter order");

    let bindings = bindings(
        r#"
        #[export]
        pub fn collision(class: i32, class_: i32) -> i32 { class + class_ }
        "#,
    );
    let error = host()
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("escaped Java parameter names must remain unique");

    assert!(
        matches!(
            &error,
            Error::JavaNameCollision { scope, name }
            if scope == "boltffi_function_demo_collision" && name == "_class"
        ),
        "{error:?}"
    );
}

#[test]
fn java_target_rejects_static_signatures_inherited_from_object() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn wait() {}
        "#,
    );
    let error = host()
        .render_with_coverage(&bindings, CoverageMode::Complete)
        .expect_err("static Java methods cannot replace Object instance methods");

    assert!(matches!(
        error,
        Error::JavaNameCollision { scope, name }
            if scope.contains("java.lang.Object") && name == "wait()"
    ));
}

#[test]
fn java_target_enforces_jvm_method_parameter_slots() {
    let overflow_parameters = std::iter::repeat_n("i64", 128)
        .enumerate()
        .map(|(index, ty)| format!("value_{index}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let overflow = bindings(&format!(
        "#[export] pub fn overflow({overflow_parameters}) {{}}"
    ));
    let error = host()
        .render_with_coverage(&overflow, CoverageMode::Complete)
        .expect_err("128 wide parameters must exceed the JVM method limit");

    assert_eq!(
        error,
        Error::UnsupportedTarget {
            target: "jvm",
            shape: "method parameter slots exceed 255 units",
        }
    );

    let overflow_symbol = overflow
        .decls()
        .iter()
        .find_map(|declaration| match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function) => Some(function.symbol().name().as_str()),
            _ => None,
        })
        .expect("overflow function symbol");
    let partial = host()
        .render_with_coverage(&overflow, CoverageMode::Partial)
        .expect("partial Java coverage must remove an invalid JVM descriptor");
    let unsupported = partial.coverage().unsupported();

    assert_eq!(unsupported.len(), 1);
    assert_eq!(
        unsupported[0].reason(),
        "method parameter slots exceed 255 units"
    );
    assert!(
        partial
            .files()
            .iter()
            .all(|file| !file.contents().contains(overflow_symbol))
    );

    let boundary_parameters = std::iter::repeat_n("i64", 127)
        .chain(std::iter::once("i32"))
        .enumerate()
        .map(|(index, ty)| format!("value_{index}: {ty}"))
        .collect::<Vec<_>>()
        .join(", ");
    let boundary = render(
        &format!("#[export] pub fn boundary({boundary_parameters}) {{}}"),
        CoverageMode::Complete,
    );

    assert!(
        java_source(&boundary, "com.boltffi.demo", "Demo").contains("public static void boundary(")
    );
}

#[test]
fn java_target_preserves_contextual_method_and_parameter_names() {
    let source = r#"
        #[export]
        pub fn record(sealed: i32, module: i32) -> i32 { sealed + module }
    "#;
    let output = render_with_host(
        source,
        CoverageMode::Complete,
        JavaHost::new("com.module.demo", "Demo")
            .expect("contextual Java identifiers")
            .version(JavaVersion::JAVA_17)
            .expect("Java 17 host")
            .desktop_loader(JavaDesktopLoader::None),
    );

    assert!(
        java_source(&output, "com.module.demo", "Demo")
            .contains("public static int record(int sealed, int module)")
    );
}

#[test]
fn java_target_derives_names_from_binding_ir_parts() {
    let source = r#"
        #[export]
        pub fn HTTPRequest(r#type: i32) -> i32 { r#type }
    "#;
    let output = render(source, CoverageMode::Complete);
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("public static int httpRequest(int type)"));
}

#[test]
fn java_target_renders_safe_function_javadoc() {
    let source = r#"
        #[doc = "Ends */ safely."]
        #[doc = "Unicode escape \\u002a\\u002f stays text."]
        #[export]
        pub fn documented(value: i32) -> i32 { value }
    "#;
    let output = render(source, CoverageMode::Complete);
    let java = java_source(&output, "com.boltffi.demo", "Demo");

    assert!(java.contains("Ends *&#47; safely."));
    assert!(java.contains("Unicode escape &#92;u002a&#92;u002f stays text."));
    assert!(java.contains("public static int documented(int value)"));
}

#[test]
fn java_host_validates_versioned_unicode_names() {
    let host = JavaHost::new("com.δοκιμή.module", "東京")
        .expect("Unicode Java names")
        .version(JavaVersion::JAVA_17)
        .expect("supported Java version");
    assert_eq!(host.java_version().release(), 17);
    assert_eq!(host.package().to_string(), "com.δοκιμή.module");
    assert_eq!(host.file().as_str(), "東京");

    let modern = JavaHost::for_version("com.𞤀.demo", "𞤀Bindings", JavaVersion::JAVA_17)
        .expect("Java 17 Unicode names");
    assert_eq!(modern.package().to_string(), "com.𞤀.demo");
    assert_eq!(modern.file().as_str(), "𞤀Bindings");

    assert!(
        JavaHost::new("com.\u{1885}.demo", "Demo")
            .unwrap()
            .version(JavaVersion::JAVA_17)
            .is_err()
    );

    assert!(JavaVersion::new(7).is_none());
    assert!(JavaVersion::new(27).is_none());
    assert!(
        JavaHost::new("com.boltffi.demo", "record")
            .unwrap()
            .version(JavaVersion::JAVA_16)
            .is_err()
    );
    assert!(
        JavaHost::new("com._.demo", "Demo")
            .unwrap()
            .version(JavaVersion::JAVA_9)
            .is_err()
    );
}

#[test]
fn generated_primitive_java_compiles_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        PRIMITIVE_FUNCTIONS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-primitives");
}

#[test]
fn generated_direct_record_java_compiles_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        DIRECT_RECORD_FUNCTIONS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-direct-records");
}

#[test]
fn generated_direct_record_calls_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        DIRECT_RECORD_CALLS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-direct-record-calls");
}

#[test]
fn generated_encoded_record_compiles_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENCODED_RECORD,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-encoded-record");
}

#[test]
fn generated_encoded_record_calls_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENCODED_RECORD_CALLS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-encoded-record-calls");
}

#[test]
fn generated_record_defaults_and_errors_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        &format!("{RECORD_DEFAULTS}\n{ERROR_RECORD}"),
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-record-semantics");
}

#[test]
fn generated_enums_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENUMS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-enums");
}

#[test]
fn generated_sealed_enums_compile_for_java_seventeen_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ENUMS,
        CoverageMode::Complete,
        JavaHost::for_version("com.boltffi.demo", "Demo", JavaVersion::JAVA_17)
            .expect("Java 17 enum host"),
    );
    compile_generated_java_for_release(&compiler, &output, "boltffi-java-sealed-enums", 17);
}

#[test]
fn generated_enum_errors_compile_for_java_eight_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        ERROR_ENUMS,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java(&compiler, &output, "boltffi-java-enum-errors");
}

#[test]
fn generated_result_api_compiles_from_an_external_package_when_available() {
    let Some(compiler) = JavaCompiler::discover() else {
        return;
    };

    let output = render_with_host(
        RESULT_RECORD,
        CoverageMode::Complete,
        JavaHost::new("com.boltffi.demo", "Demo").expect("Java host"),
    );
    compile_generated_java_with(
        &compiler,
        &output,
        "boltffi-java-result-api",
        &[(
            "consumer/ResultConsumer.java",
            r#"
                package consumer;

                import com.boltffi.demo.BoltFFIResult;
                import com.boltffi.demo.ResultHolder;

                public final class ResultConsumer {
                    private ResultConsumer() {}

                    public static ResultHolder create() {
                        return new ResultHolder(
                            BoltFFIResult.<int[], java.util.Optional<String>>ok(
                                new int[] {1, 2, 3}
                            )
                        );
                    }

                    public static boolean inspect(
                        BoltFFIResult<int[], java.util.Optional<String>> result
                    ) {
                        return result.isOk()
                            && result.okValue().length == 3
                            && result.errValue() == null;
                    }
                }
            "#,
        )],
    );
}

fn compile_generated_java(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
) {
    compile_generated_java_with(compiler, output, prefix, &[]);
}

fn compile_generated_java_with(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
) {
    compile_generated_java_with_release(compiler, output, prefix, additional_sources, None);
}

fn compile_generated_java_for_release(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    release: u16,
) {
    compile_generated_java_with_release(compiler, output, prefix, &[], Some(release));
}

fn compile_generated_java_with_release(
    compiler: &JavaCompiler,
    output: &boltffi_backend::GeneratedOutput,
    prefix: &str,
    additional_sources: &[(&str, &str)],
    release: Option<u16>,
) {
    assert!(output.files().iter().any(|file| {
        file.path().as_path() == Path::new("com/boltffi/demo/BoltFFINativeRuntime.java")
    }));
    let generated = output
        .files()
        .iter()
        .filter(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|ext| ext == "java")
        })
        .collect::<Vec<_>>();
    assert!(!generated.is_empty(), "Java target should emit source");
    let directory = temporary_directory(prefix);
    let classes = directory.join("classes");
    let mut sources = generated
        .into_iter()
        .map(|generated| {
            let source = directory.join(generated.path().as_path());
            fs::create_dir_all(source.parent().expect("generated Java parent"))
                .expect("create generated Java package");
            fs::write(&source, generated.contents()).expect("write generated Java source");
            source
        })
        .collect::<Vec<_>>();
    sources.extend(additional_sources.iter().map(|(path, contents)| {
        let source = directory.join(path);
        fs::create_dir_all(source.parent().expect("additional Java parent"))
            .expect("create additional Java package");
        fs::write(&source, contents).expect("write additional Java source");
        source
    }));
    fs::create_dir_all(&classes).expect("create Java classes directory");

    let mut javac = Command::new("javac");
    javac.args(["-encoding", "UTF-8"]);
    match release {
        Some(release) if !compiler.configure_release(&mut javac, release) => {
            fs::remove_dir_all(&directory).expect("remove unsupported Java release test directory");
            return;
        }
        Some(_) => {}
        None => compiler.configure_java_eight(&mut javac),
    }
    let compilation = javac
        .arg("-d")
        .arg(&classes)
        .args(&sources)
        .output()
        .expect("javac should execute");
    let cleanup = fs::remove_dir_all(&directory);

    assert!(
        compilation.status.success(),
        "generated Java failed to compile:\n{}",
        String::from_utf8_lossy(&compilation.stderr)
    );
    cleanup.expect("remove generated Java test directory");
}

#[test]
fn selects_java_eight_compiler_flags_from_javac_versions() {
    assert_eq!(
        JavaEightCompilation::from_version_output("javac 1.8.0_402"),
        Some(JavaEightCompilation::SourceAndTarget)
    );
    assert_eq!(
        JavaEightCompilation::from_version_output("javac 17.0.12"),
        Some(JavaEightCompilation::Release)
    );
    assert_eq!(
        JavaEightCompilation::from_version_output("javac 26-ea"),
        Some(JavaEightCompilation::Release)
    );
}

fn temporary_directory(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{}-{nonce}", std::process::id()))
}

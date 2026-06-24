use std::path::Path;

use boltffi_ast::PackageInfo;
use boltffi_backend::target::kotlin::KotlinHost;
use boltffi_binding::{Native, lower};

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

fn file(output: &boltffi_backend::GeneratedOutput, path: &str) -> String {
    output
        .files()
        .iter()
        .find(|file| file.path().as_path() == Path::new(path))
        .map(|file| file.contents())
        .map(str::to_owned)
        .expect("generated file")
}

#[test]
fn kotlin_target_renders_primitive_function_stack() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn add(left: i32, right: i32) -> i32 {
            left + right
        }
        "#,
    );
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");
    let output = target.render(&bindings).expect("Kotlin target renders");
    let kotlin = file(&output, "com/boltffi/demo/Demo.kt");

    assert!(file(&output, "jni/boltffi.h").contains("boltffi_function_demo_add"));
    assert!(file(&output, "jni/jni_glue.c").contains("Java_com_boltffi_demo_Native"));
    assert!(kotlin.contains("package com.boltffi.demo"));
    assert!(kotlin.contains("@JvmStatic external fun boltffi_function_demo_add"));
    assert!(kotlin.contains("fun add(left: Int, right: Int): Int"));
    assert!(kotlin.contains("return Native.boltffi_function_demo_add(left, right)"));
}

#[test]
fn kotlin_target_preserves_unsigned_public_api_and_native_carriers() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn widen(byte: u8, short: u16, word: u32, wide: u64) -> u32 {
            byte as u32 + short as u32 + word + wide as u32
        }
        "#,
    );
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");
    let output = target.render(&bindings).expect("Kotlin target renders");
    let kotlin = file(&output, "com/boltffi/demo/Demo.kt");

    assert!(
        kotlin.contains(
            "@JvmStatic external fun boltffi_function_demo_widen(byte: Byte, short_: Short, word: Int, wide: Long): Int"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("fun widen(byte: UByte, short: UShort, word: UInt, wide: ULong): UInt"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains(
            "return Native.boltffi_function_demo_widen(byte.toByte(), short.toShort(), word.toInt(), wide.toLong()).toUInt()"
        ),
        "{kotlin}"
    );
}

#[test]
fn kotlin_target_encodes_string_and_bytes_parameters_with_pooled_writers() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn name_length(name: String) -> u32 {
            name.len() as u32
        }

        #[export]
        pub fn body_length(body: Vec<u8>) -> u32 {
            body.len() as u32
        }
        "#,
    );
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");
    let output = target.render(&bindings).expect("Kotlin target renders");
    let kotlin = file(&output, "com/boltffi/demo/Demo.kt");

    assert!(
        kotlin.contains(
            "@JvmStatic external fun boltffi_function_demo_name_length(name: ByteArray): Int"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("fun nameLength(name: String): UInt"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains(
            "val __boltffi_name_wire = WireWriterPool.acquire(4 + Utf8Codec.maxBytes(name))"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("__boltffi_name_writer.writeString(name)"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains(
            "return Native.boltffi_function_demo_name_length(__boltffi_name_wire.bytes()).toUInt()"
        ),
        "{kotlin}"
    );
    assert!(kotlin.contains("__boltffi_name_wire.close()"), "{kotlin}");

    assert!(
        kotlin.contains(
            "@JvmStatic external fun boltffi_function_demo_body_length(body: ByteArray): Int"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("fun bodyLength(body: ByteArray): UInt"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("val __boltffi_body_wire = WireWriterPool.acquire(4 + body.size)"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("__boltffi_body_writer.writeBytes(body)"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains(
            "return Native.boltffi_function_demo_body_length(__boltffi_body_wire.bytes()).toUInt()"
        ),
        "{kotlin}"
    );
    assert!(kotlin.contains("__boltffi_body_wire.close()"), "{kotlin}");
}

#[test]
fn kotlin_target_decodes_string_and_bytes_returns_through_wire_reader() {
    let bindings = bindings(
        r#"
        #[export]
        pub fn greeting() -> String {
            "hello".to_owned()
        }

        #[export]
        pub fn payload() -> Vec<u8> {
            vec![1, 2, 3]
        }
        "#,
    );
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");
    let output = target.render(&bindings).expect("Kotlin target renders");
    let kotlin = file(&output, "com/boltffi/demo/Demo.kt");

    assert!(
        kotlin.contains("@JvmStatic external fun boltffi_function_demo_greeting(): ByteArray?"),
        "{kotlin}"
    );
    assert!(kotlin.contains("fun greeting(): String"), "{kotlin}");
    assert!(
        kotlin.contains(
            "val __boltffi_result = Native.boltffi_function_demo_greeting() ?: throw IllegalStateException(\"null buffer returned\")"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("val __boltffi_reader = WireReader(__boltffi_result)"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("return __boltffi_reader.readString()"),
        "{kotlin}"
    );

    assert!(
        kotlin.contains("@JvmStatic external fun boltffi_function_demo_payload(): ByteArray?"),
        "{kotlin}"
    );
    assert!(kotlin.contains("fun payload(): ByteArray"), "{kotlin}");
    assert!(
        kotlin.contains(
            "val __boltffi_result = Native.boltffi_function_demo_payload() ?: throw IllegalStateException(\"null buffer returned\")"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("return __boltffi_reader.readBytes()"),
        "{kotlin}"
    );
}

#[test]
fn kotlin_target_renders_direct_records_and_function_bridges() {
    let bindings = bindings(
        r#"
        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: f64,
            pub y: f64,
        }

        #[repr(u8)]
        #[data]
        pub enum Mode {
            Fast = 1,
            Slow = 2,
        }

        #[export]
        pub fn origin() -> Point {
            Point { x: 0.0, y: 0.0 }
        }

        #[export]
        pub fn magnitude(point: Point) -> f64 {
            point.x.hypot(point.y)
        }

        #[export]
        pub fn echo_mode(mode: Mode) -> Mode {
            mode
        }
        "#,
    );
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");
    let output = target.render(&bindings).expect("Kotlin target renders");
    let kotlin = file(&output, "com/boltffi/demo/Demo.kt");

    assert!(kotlin.contains("data class Point("), "{kotlin}");
    assert!(kotlin.contains("val x: Double,"), "{kotlin}");
    assert!(kotlin.contains("val y: Double"), "{kotlin}");
    assert!(
        kotlin.contains(".allocate(16)")
            && kotlin.contains(".order(java.nio.ByteOrder.nativeOrder())"),
        "{kotlin}"
    );
    assert!(kotlin.contains("buffer.putDouble(0, x)"), "{kotlin}");
    assert!(kotlin.contains("buffer.putDouble(8, y)"), "{kotlin}");
    assert!(kotlin.contains("buffer.getDouble(0)"), "{kotlin}");
    assert!(kotlin.contains("buffer.getDouble(8)"), "{kotlin}");

    assert!(
        kotlin.contains("@JvmStatic external fun boltffi_function_demo_origin(): ByteArray?"),
        "{kotlin}"
    );
    assert!(kotlin.contains("fun origin(): Point"), "{kotlin}");
    assert!(
        kotlin.contains("return Point.fromByteArray(__boltffi_result)"),
        "{kotlin}"
    );

    assert!(
        kotlin.contains(
            "@JvmStatic external fun boltffi_function_demo_magnitude(point: ByteArray): Double"
        ),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("fun magnitude(point: Point): Double"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("return Native.boltffi_function_demo_magnitude(point.toByteArray())"),
        "{kotlin}"
    );

    assert!(
        kotlin.contains("enum class Mode(val value: UByte)"),
        "{kotlin}"
    );
    assert!(kotlin.contains("Fast(1.toUByte()),"), "{kotlin}");
    assert!(kotlin.contains("Slow(2.toUByte());"), "{kotlin}");
    assert!(
        kotlin.contains("fun fromValue(value: UByte): Mode"),
        "{kotlin}"
    );
    assert!(
        kotlin
            .contains("@JvmStatic external fun boltffi_function_demo_echo_mode(mode: Byte): Byte"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains("fun echoMode(mode: Mode): Mode"),
        "{kotlin}"
    );
    assert!(
        kotlin.contains(
            "return Mode.fromValue(Native.boltffi_function_demo_echo_mode(mode.value.toByte()).toUByte())"
        ),
        "{kotlin}"
    );
}

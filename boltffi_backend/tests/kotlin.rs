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

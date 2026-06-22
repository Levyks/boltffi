use boltffi_ast::PackageInfo;
use boltffi_binding::{Native, lower};
use std::{fs, path::PathBuf};

use boltffi_backend::{
    bridge::{
        c::CBridge,
        jni::{JniBridge, JniBridgeContract},
    },
    core::{BridgeLayer, BridgeOutput, BridgeStack},
};

#[path = "jni/associated.rs"]
mod associated;
#[path = "jni/callback.rs"]
mod callback;
#[path = "jni/callback_return.rs"]
mod callback_return;
#[path = "jni/constant.rs"]
mod constant;
#[path = "jni/direct_vector.rs"]
mod direct_vector;
#[path = "jni/native_methods.rs"]
mod native_methods;
#[path = "jni/stream.rs"]
mod stream;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

pub fn bridge(source: &str) -> BridgeOutput<JniBridgeContract> {
    let bindings = bindings(source);
    let stack = BridgeLayer::new(
        CBridge::new("jni/demo.h").expect("C header bridge"),
        JniBridge::new("com.boltffi.demo", "Native", "jni/jni_glue.c").expect("JNI bridge"),
    );
    stack.build(&bindings).expect("JNI bridge stack")
}

pub fn files(source: &str) -> Vec<(String, String)> {
    bridge(source)
        .output()
        .files()
        .iter()
        .map(|file| {
            (
                file.path().as_path().display().to_string(),
                file.contents().to_owned(),
            )
        })
        .collect()
}

pub fn rendered_fixture(name: &str) -> String {
    rendered_files(&files(&fixture(name)))
}

pub fn bridge_fixture(name: &str) -> BridgeOutput<JniBridgeContract> {
    bridge(&fixture(name))
}

fn fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("source fixture")
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("source")
        .join(format!("{name}.rs"))
}

fn rendered_files(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, contents)| format!("===== {path} =====\n{contents}"))
        .collect::<Vec<_>>()
        .join("\n")
}

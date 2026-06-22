use boltffi_ast::PackageInfo;
use boltffi_binding::{Native, lower};

use crate::{
    bridge::{
        c::CBridge,
        jni::{JniBridge, JniBridgeContract},
    },
    core::{BridgeLayer, BridgeOutput, BridgeStack},
};

mod associated;
mod callback;
mod constant;
mod core;
mod direct_vector;
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

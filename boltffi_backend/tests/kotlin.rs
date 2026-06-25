use std::{fs, path::PathBuf};

use boltffi_ast::PackageInfo;
use boltffi_backend::target::kotlin::KotlinHost;
use boltffi_binding::{Native, lower};

#[path = "kotlin/callback.rs"]
mod callback;
#[path = "kotlin/constant.rs"]
mod constant;
#[path = "kotlin/direct_vector.rs"]
mod direct_vector;
#[path = "kotlin/exports.rs"]
mod exports;
#[path = "kotlin/stream.rs"]
mod stream;

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

pub fn rendered_fixture(name: &str) -> String {
    let kotlin_file = files(&fixture(name))
        .into_iter()
        .find(|(path, _)| path.ends_with(".kt"))
        .expect("Kotlin target should render a Kotlin source file");
    rendered_files(&[kotlin_file])
}

pub fn files(source: &str) -> Vec<(String, String)> {
    let bindings = bindings(source);
    let target = KotlinHost::new("com.boltffi.demo", "Demo")
        .expect("Kotlin host")
        .into_target()
        .expect("Kotlin target");

    target
        .render(&bindings)
        .expect("Kotlin target renders")
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

pub fn fixture(name: &str) -> String {
    fs::read_to_string(fixture_path(name)).expect("source fixture")
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("source")
        .join(format!("{name}.rs"))
}

pub fn rendered_files(files: &[(String, String)]) -> String {
    files
        .iter()
        .map(|(path, contents)| format!("===== {path} =====\n{contents}"))
        .collect::<Vec<_>>()
        .join("\n")
}

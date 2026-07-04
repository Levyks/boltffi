use std::{fs, path::PathBuf};

use boltffi_ast::PackageInfo;
use boltffi_backend::target::swift::SwiftHost;
use boltffi_binding::{Native, lower};

fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Native>(&source).expect("fixture should lower")
}

fn rendered_fixture(name: &str) -> String {
    let host = SwiftHost::new("DemoFFI").expect("Swift host");
    let bindings = bindings(&fixture(name));
    let target = host.into_target().expect("Swift target");
    let output = target.render(&bindings).expect("Swift target renders");
    let swift_file = output
        .files()
        .iter()
        .find(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|extension| extension == "swift")
        })
        .expect("Swift target should render a Swift source file");
    format!(
        "===== {} =====\n{}",
        swift_file.path().as_path().display(),
        swift_file.contents()
    )
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

#[test]
fn swift_target_renders_primitive_function_stack() {
    insta::assert_snapshot!(rendered_fixture("exports/single_function"));
}

#[test]
fn swift_target_renders_direct_records_and_c_style_enums() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_records_and_c_style_enums"));
}

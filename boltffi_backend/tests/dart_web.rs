use boltffi_ast::PackageInfo;
use boltffi_backend::{
    core::{GeneratedFile, GeneratedOutput},
    target::dart_web::DartWebHost,
};
use boltffi_binding::{Wasm32, lower};

mod source;

use source::SourceFixture;

fn bindings(source: &str) -> boltffi_binding::Bindings<Wasm32> {
    let file = syn::parse_str(source).expect("valid source fixture");
    let source =
        boltffi_scan::scan_file(file, PackageInfo::new("demo", None)).expect("fixture should scan");
    lower::<Wasm32>(&source).expect("fixture should lower")
}

fn rendered_fixture(name: &str) -> String {
    rendered_source(SourceFixture::one(name))
}

fn rendered_source(fixture: SourceFixture) -> String {
    rendered_output(fixture)
}

fn rendered_output(fixture: SourceFixture) -> String {
    let bindings = bindings(&fixture.read());
    let host = DartWebHost::new("demo").expect("dart_web host");
    let target = host.into_target();
    let output = target.render(&bindings).expect("dart_web target renders");
    rendered_dart_file(&output)
}

fn rendered_dart_file(output: &GeneratedOutput) -> String {
    let dart_file = dart_file(output);
    format!(
        "===== {} =====\n{}",
        dart_file.path().as_path().display(),
        dart_file.contents()
    )
}

fn dart_file(output: &GeneratedOutput) -> &GeneratedFile {
    output
        .files()
        .iter()
        .find(|file| {
            file.path()
                .as_path()
                .extension()
                .is_some_and(|extension| extension == "dart")
        })
        .expect("dart_web target should render a Dart source file")
}

#[test]
fn dart_web_target_renders_callback_js_wrapper() {
    insta::assert_snapshot!(rendered_fixture("callback/foreign_callback_parameter"));
}

#[test]
fn dart_web_target_renders_fallible_callback_js_wrapper() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_status_result"));
}

#[test]
fn dart_web_target_renders_async_callback_js_wrapper() {
    insta::assert_snapshot!(rendered_fixture("callback/async_fallible_callback"));
}

#[test]
fn dart_web_target_callback_to_js_checks_for_js_wrapper_fast_path() {
    let rendered = rendered_fixture("callback/foreign_callback_parameter");
    assert!(
        rendered.contains("is ListenerJsWrapper"),
        "expected to_js() to branch on the generated JsWrapper type, got:\n{rendered}"
    );
    assert!(
        rendered.contains("__boltffiCallback._js"),
        "expected the fast path to hand the wrapped JSObject straight through, got:\n{rendered}"
    );
}

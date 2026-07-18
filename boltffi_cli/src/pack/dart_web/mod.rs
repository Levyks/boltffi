//! Packages the dart-web (js_interop over wasm) target into something a
//! browser can actually load: builds the wasm binary, generates the
//! `target::typescript` module (which already has full, tested marshalling
//! for classes/callbacks/async/records -- everything `target::dart_web`'s
//! `@JS()` bindings assume), and type-erases it into plain JS via Node's
//! `node:module` `stripTypeScriptTypes` API. That API needs only a `node`
//! binary -- no npm, no network, no `tsc` -- so this has no toolchain
//! dependency beyond Node itself.
//!
//! The vendored `@boltffi/runtime` TS sources are embedded in this binary
//! (`include_str!`) rather than read from the boltffi repo checkout, since
//! `boltffi_cli` runs from an arbitrary consumer project, not this repo.

use std::path::{Path, PathBuf};
use std::process::Command;

use boltffi_bindgen::target::Target;

use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackDartOptions;
use crate::config::{Config, WasmProfile};
use crate::pack::resolve_build_cargo_args;
use crate::pack::wasm::{WasmArtifactPath, build_wasm_target};
use crate::reporter::Reporter;

/// One `@boltffi/runtime` source file, embedded at compile time. Order
/// matters only for readability -- each file resolves its own sibling
/// imports (`./wire.js`, etc.) once stripped, independent of write order.
const RUNTIME_SOURCES: &[(&str, &str)] = &[
    (
        "wire.ts",
        include_str!("../../../../runtime/typescript/src/wire.ts"),
    ),
    (
        "callback.ts",
        include_str!("../../../../runtime/typescript/src/callback.ts"),
    ),
    (
        "stream.ts",
        include_str!("../../../../runtime/typescript/src/stream.ts"),
    ),
    (
        "module.ts",
        include_str!("../../../../runtime/typescript/src/module.ts"),
    ),
    (
        "index.ts",
        include_str!("../../../../runtime/typescript/src/index.ts"),
    ),
];

/// Runs `pack dart-web`'s browser-asset pipeline as a step of `pack dart`
/// (called when `[targets.wasm]` is enabled) or standalone. Produces, inside
/// the dart-web package's `lib/src/web/` directory: the type-stripped
/// compiled module, the vendored runtime it imports, and a copy of the wasm
/// binary -- everything the generated `..._web_loader.mjs` (see
/// `boltffi_backend::target::dart_web`) imports by relative path.
pub(crate) fn pack_dart_web_assets(
    config: &Config,
    options: &PackDartOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_wasm_enabled() {
        return Ok(());
    }
    if !config.should_process(Target::DartWeb, options.experimental) {
        return Ok(());
    }

    reporter.section("🕸️", "Packing Dart Web (wasm + JS)");

    let node = locate_node()?;

    let requested_profile = if options.execution.release {
        WasmProfile::Release
    } else {
        config.wasm_profile()
    };
    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);

    if !options.execution.no_build {
        let step = reporter.step("Building WASM target");
        build_wasm_target(config, requested_profile, &build_cargo_args, &step)?;
        step.finish_success();
    }

    let wasm_artifact_path = WasmArtifactPath::resolve(config, requested_profile)?.into_path();
    if !wasm_artifact_path.exists() {
        return Err(CliError::FileNotFound(wasm_artifact_path));
    }

    let package_name = config.dart_package_name();
    let web_dir = config.dart_output().join(&package_name).join("lib/src/web");
    std::fs::create_dir_all(&web_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: web_dir.clone(),
        source,
    })?;

    let step = reporter.step("Generating TypeScript bindings for browser");
    let typescript_output = config.wasm_typescript_output().join("dart_web_module");
    run_generate_with_output(
        config,
        GenerateOptions {
            target: GenerateTarget::Typescript,
            output: Some(typescript_output.clone()),
            experimental: false,
            ir: true,
            cargo_args: build_cargo_args.clone(),
        },
    )?;
    step.finish_success();

    let runtime_dir = web_dir.join("boltffi_runtime");
    let step = reporter.step("Stripping TypeScript types (Node)");
    strip_runtime(&node, &runtime_dir)?;
    let generated_ts = typescript_output.join(format!("{package_name}.ts"));
    if !generated_ts.exists() {
        return Err(CliError::FileNotFound(generated_ts));
    }
    let module_js = web_dir.join(format!("{package_name}.js"));
    strip_file(&node, &generated_ts, &module_js)?;
    step.finish_success();

    let step = reporter.step("Copying WASM binary");
    let packaged_wasm = web_dir.join(format!("{package_name}_bg.wasm"));
    std::fs::copy(&wasm_artifact_path, &packaged_wasm).map_err(|source| CliError::CopyFailed {
        from: wasm_artifact_path,
        to: packaged_wasm,
        source,
    })?;
    step.finish_success();

    reporter.finish();
    Ok(())
}

fn strip_runtime(node: &Path, runtime_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(runtime_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: runtime_dir.to_path_buf(),
        source,
    })?;
    for (name, source) in RUNTIME_SOURCES {
        let stripped = strip_typescript(node, source)?;
        let out_name = name.replace(".ts", ".js");
        let out_path = runtime_dir.join(&out_name);
        std::fs::write(&out_path, stripped).map_err(|source| CliError::WriteFailed {
            path: out_path,
            source,
        })?;
    }
    Ok(())
}

fn strip_file(node: &Path, source_file: &Path, out_file: &Path) -> Result<()> {
    let source =
        std::fs::read_to_string(source_file).map_err(|source_err| CliError::CommandFailed {
            command: format!("reading {}: {}", source_file.display(), source_err),
            status: None,
        })?;
    // The wasm/TS backend's `runtime_package` import needs to resolve to
    // our vendored, stripped copy via a plain relative ESM specifier --
    // real npm resolution isn't available (and isn't a browser feature).
    let source = source.replace("\"@boltffi/runtime\"", "\"./boltffi_runtime/index.js\"");
    let stripped = strip_typescript(node, &source)?;
    std::fs::write(out_file, stripped).map_err(|source| CliError::WriteFailed {
        path: out_file.to_path_buf(),
        source,
    })
}

/// Runs Node's `node:module` `stripTypeScriptTypes(source, { mode:
/// "transform" })` on `source`, returning the resulting JS text. `transform`
/// mode (not the default `strip` mode) is required because the runtime
/// uses `const enum` and TS parameter-property constructors, neither of
/// which the strip-only fast path supports.
fn strip_typescript(node: &Path, source: &str) -> Result<String> {
    let mut command = Command::new(node);
    command.args(["-e", STRIP_SCRIPT]);
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let mut child = command.spawn().map_err(|_| CliError::CommandFailed {
        command: format!("{}", node.display()),
        status: None,
    })?;

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("stdin was piped");
        stdin
            .write_all(source.as_bytes())
            .map_err(|_| CliError::CommandFailed {
                command: "node (writing TypeScript source to stdin)".to_string(),
                status: None,
            })?;
    }

    let output = child
        .wait_with_output()
        .map_err(|_| CliError::CommandFailed {
            command: format!("{}", node.display()),
            status: None,
        })?;

    if !output.status.success() {
        return Err(CliError::CommandFailed {
            command: format!(
                "node stripTypeScriptTypes failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            status: output.status.code(),
        });
    }

    String::from_utf8(output.stdout).map_err(|_| CliError::CommandFailed {
        command: "node stripTypeScriptTypes produced non-UTF-8 output".to_string(),
        status: None,
    })
}

const STRIP_SCRIPT: &str = r#"
const { stripTypeScriptTypes } = require('node:module');
const chunks = [];
process.stdin.on('data', (chunk) => chunks.push(chunk));
process.stdin.on('end', () => {
  const source = Buffer.concat(chunks).toString('utf8');
  process.stdout.write(stripTypeScriptTypes(source, { mode: 'transform' }));
});
"#;

fn locate_node() -> Result<PathBuf> {
    if let Ok(path) = which::which("node") {
        return Ok(path);
    }
    Err(CliError::CommandFailed {
        command:
            "node not found in PATH -- `pack dart-web` needs a Node.js binary (v22.6+) to type-erase the generated TypeScript bindings via `node:module`'s `stripTypeScriptTypes`. Install Node.js and ensure it's on PATH."
                .to_string(),
        status: None,
    })
}

//! Packages the dart-web (js_interop over wasm) target into something a
//! browser can actually load: builds the wasm binary, generates the
//! `target::typescript` module (which already has full, tested marshalling
//! for classes/callbacks/async/records -- everything `target::dart_web`'s
//! `@JS()` bindings assume), and converts it to plain JS.

use std::path::{Path, PathBuf};

use boltffi_bindgen::target::Target;

use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackDartOptions;
use crate::config::{Config, WasmProfile};
use crate::pack::resolve_build_cargo_args;
use crate::pack::wasm::{WasmArtifactPath, build_wasm_target};
use crate::reporter::Reporter;

/// Pre-stripped runtime JS files embedded directly in the binary.
const RUNTIME_SOURCES: &[(&str, &str)] = &[
    ("wire.js", include_str!("runtime/wire.js")),
    ("callback.js", include_str!("runtime/callback.js")),
    ("stream.js", include_str!("runtime/stream.js")),
    ("module.js", include_str!("runtime/module.js")),
    ("index.js", include_str!("runtime/index.js")),
];

/// Runs `pack dart-web`'s browser-asset pipeline as a step of `pack dart`
/// (called when `[targets.wasm]` is enabled) or standalone. Produces, inside
/// the dart-web package's `lib/src/web/` directory: the compiled module,
/// the vendored runtime it imports, and a copy of the wasm binary.
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

    let step = reporter.step("Generating web module bindings");
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
    let step = reporter.step("Packaging JavaScript runtime");
    write_runtime(&runtime_dir)?;
    let generated_ts = typescript_output.join(format!("{package_name}.ts"));
    if !generated_ts.exists() {
        return Err(CliError::FileNotFound(generated_ts));
    }
    let module_js = web_dir.join(format!("{package_name}.js"));
    strip_file(&generated_ts, &module_js)?;
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

fn write_runtime(runtime_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(runtime_dir).map_err(|source| CliError::CreateDirectoryFailed {
        path: runtime_dir.to_path_buf(),
        source,
    })?;
    for (name, source) in RUNTIME_SOURCES {
        let out_path = runtime_dir.join(name);
        std::fs::write(&out_path, source).map_err(|source| CliError::WriteFailed {
            path: out_path,
            source,
        })?;
    }
    Ok(())
}

fn locate_node() -> Option<PathBuf> {
    which::which("node").ok()
}

fn strip_file(source_file: &Path, out_file: &Path) -> Result<()> {
    let source =
        std::fs::read_to_string(source_file).map_err(|source_err| CliError::CommandFailed {
            command: format!("reading {}: {}", source_file.display(), source_err),
            status: None,
        })?;
    let source = source.replace("\"@boltffi/runtime\"", "\"./boltffi_runtime/index.js\"");

    let stripped = if let Some(node) = locate_node() {
        match strip_typescript_with_node(&node, &source) {
            Ok(js) => js,
            Err(_) => strip_typescript_in_rust(&source),
        }
    } else {
        strip_typescript_in_rust(&source)
    };

    std::fs::write(out_file, stripped).map_err(|source| CliError::WriteFailed {
        path: out_file.to_path_buf(),
        source,
    })
}

fn strip_typescript_with_node(node: &Path, source: &str) -> std::result::Result<String, String> {
    use std::io::Write;
    let script = r#"
const fs = require('fs');
const { stripTypeScriptTypes } = require('node:module');
const code = fs.readFileSync(0, 'utf8');
process.stdout.write(stripTypeScriptTypes(code));
"#;

    let mut child = std::process::Command::new(node)
        .arg("-e")
        .arg(script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(source.as_bytes());
    }

    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

fn strip_typescript_in_rust(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut in_interface = false;
    let mut brace_depth: usize = 0;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("import type ") {
            continue;
        }
        if trimmed.starts_with("export type ") || trimmed.starts_with("type ") {
            continue;
        }
        if trimmed.starts_with("export interface ") || trimmed.starts_with("interface ") {
            in_interface = true;
            let adds = line.chars().filter(|&c| c == '{').count();
            let subs = line.chars().filter(|&c| c == '}').count();
            brace_depth = brace_depth.saturating_add(adds).saturating_sub(subs);
            if brace_depth == 0 {
                in_interface = false;
            }
            continue;
        }
        if in_interface {
            let adds = line.chars().filter(|&c| c == '{').count();
            let subs = line.chars().filter(|&c| c == '}').count();
            brace_depth = brace_depth.saturating_add(adds).saturating_sub(subs);
            if brace_depth == 0 {
                in_interface = false;
            }
            continue;
        }

        let line = strip_ts_annotations(line);
        out.push_str(&line);
        out.push('\n');
    }

    out
}

fn strip_ts_annotations(line: &str) -> String {
    let mut result = line.to_string();

    result = result.replace(" as any", "");
    result = result.replace(" as const", "");
    result = result.replace(" as WireReader", "");
    result = result.replace(" as WireWriter", "");
    result = result.replace(" as number", "");
    result = result.replace(" as string", "");
    result = result.replace(" as bigint", "");
    result = result.replace(" as boolean", "");
    result = result.replace(" as Uint8Array", "");
    result = result.replace(" as ArrayBuffer", "");

    while let Some(pos) = result.find(" as WireResult<") {
        if let Some(end) = result[pos..].find('>') {
            result.replace_range(pos..pos + end + 1, "");
        } else {
            break;
        }
    }

    result = result.replace("private readonly ", "");
    result = result.replace("private ", "");
    result = result.replace("public ", "");
    result = result.replace("protected ", "");
    result = result.replace("readonly ", "");

    while let Some(pos) = result.find(": WireCodec<") {
        if let Some(end) = result[pos..].find('>') {
            result.replace_range(pos..pos + end + 1, "");
        } else {
            break;
        }
    }

    let is_decl = result.trim().starts_with("let ")
        || result.trim().starts_with("const ")
        || result.trim().starts_with("var ");
    if is_decl {
        let colon_pos = result.find(':');
        if let Some(colon) = colon_pos {
            if let Some(eq) = result[colon..].find('=') {
                result.replace_range(colon..colon + eq, " ");
            } else if let Some(semi) = result[colon..].find(';') {
                result.replace_range(colon..colon + semi, "");
            }
        }
    }

    result = result.replace(": number", "");
    result = result.replace(": string", "");
    result = result.replace(": boolean", "");
    result = result.replace(": bigint", "");
    result = result.replace(": void", "");
    result = result.replace(": Uint8Array", "");
    result = result.replace(": ArrayBuffer", "");
    result = result.replace(": Error", "");
    result = result.replace(": unknown[]", "");
    result = result.replace(": BufferSource | Response", "");

    if let Some(pos) = result.find("): ") {
        if let Some(brace) = result[pos..].find('{') {
            result.replace_range(pos + 1..pos + brace, "");
        } else if let Some(arrow) = result[pos..].find("=>") {
            result.replace_range(pos + 1..pos + arrow, " ");
        }
    }

    result
}

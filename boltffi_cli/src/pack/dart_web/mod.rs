//! Packages the dart-web (js_interop over wasm) target into something a
//! browser can actually load: builds the wasm binary, generates the
//! `target::typescript` module (which already has full, tested marshalling
//! for classes/callbacks/async/records -- everything `target::dart_web`'s
//! `@JS()` bindings assume), and compiles it to plain JS via the same
//! `tsc`-based pipeline `pack wasm` already uses (see
//! `pack::wasm::transpile_typescript_bundle`) -- no separate toolchain.

use std::path::Path;

use boltffi_bindgen::target::Target;

use crate::cli::{CliError, Result};
use crate::commands::generate::{GenerateOptions, GenerateTarget, run_generate_with_output};
use crate::commands::pack::PackDartOptions;
use crate::config::{Config, WasmProfile};
use crate::pack::resolve_build_cargo_args;
use crate::pack::wasm::{WasmArtifactPath, build_wasm_target, transpile_typescript_bundle};
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
    let dart_enabled = config.should_process(Target::Dart, options.experimental);
    let dart_web_enabled = config.should_process(Target::DartWeb, options.experimental);
    if !dart_enabled && !dart_web_enabled {
        return Ok(());
    }

    reporter.section("🕸️", "Packing Dart Web (wasm + JS)");

    let requested_wasm_profile = if options.execution.release {
        WasmProfile::Release
    } else {
        config.wasm_profile()
    };
    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let build_profile = crate::build::resolve_build_profile(
        matches!(requested_wasm_profile, WasmProfile::Release),
        &build_cargo_args,
    );

    let wasm_artifact_profile = match build_profile {
        crate::build::CargoBuildProfile::Debug => WasmProfile::Debug,
        crate::build::CargoBuildProfile::Release => WasmProfile::Release,
        crate::build::CargoBuildProfile::Named(_) if config.wasm_has_artifact_path_override() => {
            requested_wasm_profile
        }
        crate::build::CargoBuildProfile::Named(profile_name) => {
            return Err(CliError::CommandFailed {
                command: format!(
                    "custom cargo profile '{}' for wasm pack requires targets.wasm.artifact_path",
                    profile_name
                ),
                status: None,
            });
        }
    };

    if !options.execution.no_build {
        let step = reporter.step("Building WASM target");
        build_wasm_target(config, requested_wasm_profile, &build_cargo_args, &step)?;
        step.finish_success();
    }

    let wasm_artifact_path = WasmArtifactPath::resolve(config, wasm_artifact_profile)?.into_path();
    if !wasm_artifact_path.exists() {
        return Err(CliError::FileNotFound(wasm_artifact_path));
    }

    let package_name = config.dart_package_name();
    let module_name = config.wasm_typescript_module_name();
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
    step.finish_success();

    let generated_ts = typescript_output.join(format!("{module_name}.ts"));
    if !generated_ts.exists() {
        return Err(CliError::FileNotFound(generated_ts));
    }
    let step = reporter.step("Compiling TypeScript bindings");
    rewrite_runtime_import(&generated_ts)?;
    transpile_typescript_bundle(config, &generated_ts, &web_dir)?;
    step.finish_success();

    let step = reporter.step("Copying WASM binary");
    let packaged_wasm = web_dir.join(format!("{module_name}_bg.wasm"));
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

/// `transpile_typescript_bundle` compiles as-is; it doesn't rewrite
/// imports. The compiled module's `@boltffi/runtime` import needs to
/// resolve to our vendored, locally-written copy via a plain relative ESM
/// specifier instead -- real npm resolution isn't available in a browser
/// (or desired here, per `write_runtime`'s pre-baked runtime files).
fn rewrite_runtime_import(generated_ts: &Path) -> Result<()> {
    let source =
        std::fs::read_to_string(generated_ts).map_err(|source| CliError::CommandFailed {
            command: format!("reading {}: {}", generated_ts.display(), source),
            status: None,
        })?;
    let rewritten = source.replace("\"@boltffi/runtime\"", "\"./boltffi_runtime/index.js\"");
    std::fs::write(generated_ts, rewritten).map_err(|source| CliError::WriteFailed {
        path: generated_ts.to_path_buf(),
        source,
    })
}

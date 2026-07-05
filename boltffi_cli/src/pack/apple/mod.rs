mod names;
mod spm;
mod xcframework;

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};

use boltffi_backend::GeneratedOutput;
use boltffi_bindgen::generate::{Generation, GenerationError};
use boltffi_bindgen::target::Target as BindgenTarget;
use boltffi_binding::{
    BINDING_EXPANSION_BUILD_ENV, BINDING_EXPANSION_ROOT_ENV, BINDING_EXPANSION_SOURCE_ENV,
    BINDING_EXPANSION_SURFACE_ENV, BindingMetadataSurface,
};

use crate::build::{BuildOptions, Builder, OutputCallback, all_successful, failed_targets};
use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::commands::pack::PackAppleOptions;
use crate::config::{Config, DebugSymbolsBundle, DebugSymbolsFormat, SpmDistribution, SpmLayout};
use crate::pack::PackError;
use crate::pack::symbols::{
    DebugSymbolArtifact, DebugSymbolArtifactKind, ensure_debug_symbols_profile_has_debuginfo,
    ensure_existing_debug_symbol_artifacts_are_usable, write_debug_symbols_zip,
};
use crate::reporter::Reporter;
use crate::target::{BuiltLibrary, Platform};

use super::{
    discover_built_libraries_for_targets, missing_built_libraries, print_cargo_line,
    resolve_build_cargo_args, scratch,
};

pub(crate) use self::spm::SpmPackageGenerator;
pub(crate) use self::xcframework::{XcframeworkBuilder, compute_checksum};

const BINDING_EXPANSION_CFG: &str = "boltffi_binding_expansion";

pub(crate) fn pack_apple(
    config: &Config,
    options: PackAppleOptions,
    reporter: &Reporter,
) -> Result<()> {
    if !config.is_apple_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.apple.enabled = false".to_string(),
            status: None,
        });
    }

    reporter.section("🍎", "Packing Apple");

    if !config.apple_include_macos() {
        reporter.warning("macOS excluded (targets.apple.include_macos = false)");
    }

    if options.spm_only && options.xcframework_only {
        return Err(CliError::CommandFailed {
            command: "cannot combine --spm-only and --xcframework-only".to_string(),
            status: None,
        });
    }

    let build_cargo_args = resolve_build_cargo_args(config, &options.execution.cargo_args);
    let selected_crate = AppleCrateSelection::resolve(config, &build_cargo_args)?;
    let build_profile =
        crate::build::resolve_build_profile(options.execution.release, &build_cargo_args);
    let apple_targets = config.apple_targets();

    if !options.execution.no_build {
        if config.apple_debug_symbols_enabled() {
            ensure_debug_symbols_profile_has_debuginfo(
                &build_cargo_args,
                &build_profile,
                "targets.apple.debug_symbols",
                &apple_targets
                    .iter()
                    .map(|target| target.triple().to_string())
                    .collect::<Vec<_>>(),
            )?;
        }
        let step = reporter.step("Building Apple targets");
        build_apple_targets(
            config,
            &apple_targets,
            options.execution.release,
            &build_cargo_args,
            &selected_crate,
            &step,
        )?;
        step.finish_success();
    }

    let layout = options.layout.unwrap_or_else(|| config.apple_spm_layout());
    let package_root = config.apple_spm_output();
    let scratch_directory = scratch::Directory::for_target("apple")?;
    let headers_dir = scratch_directory.join("headers");

    if options.execution.regenerate {
        scratch_directory.recreate()?;
        let step = reporter.step("Generating Apple bindings");
        generate_apple_bindings(config, layout, &package_root, &headers_dir, &selected_crate)?;
        step.finish_success();
    }

    let libraries = discover_built_libraries_for_targets(
        &config.crate_artifact_name(),
        build_profile.output_directory_name(),
        &apple_targets,
    )?;
    let apple_libraries: Vec<_> = libraries
        .into_iter()
        .filter(|library| library.target.platform().is_apple())
        .collect();

    let missing_targets = missing_built_libraries(&apple_targets, &apple_libraries);
    if !missing_targets.is_empty() {
        return Err(PackError::MissingBuiltLibraries {
            platform: "Apple".to_string(),
            targets: missing_targets,
        }
        .into());
    }

    if options.execution.no_build && config.apple_debug_symbols_enabled() {
        ensure_existing_debug_symbol_artifacts_are_usable(
            &apple_libraries
                .iter()
                .map(|library| library.path.clone())
                .collect::<Vec<_>>(),
            "targets.apple.debug_symbols",
        )?;
    }

    if !headers_dir.exists() {
        return Err(CliError::FileNotFound(headers_dir));
    }

    let should_build_xcframework = !options.spm_only;
    let should_generate_spm = !options.xcframework_only;

    let xcframework_output = if should_build_xcframework {
        let step = reporter.step("Creating xcframework");
        let output = XcframeworkBuilder::new(
            config,
            apple_libraries.clone(),
            headers_dir.clone(),
            scratch_directory.join("xcframework"),
        )
        .build_with_zip()?;
        step.finish_success();
        Some(output)
    } else {
        None
    };

    if config.apple_debug_symbols_enabled() {
        let step = reporter.step("Bundling Apple debug symbols");
        write_apple_debug_symbols(config, &apple_libraries)?;
        step.finish_success();
    }

    if should_generate_spm {
        let (checksum, version) = match config.apple_spm_distribution() {
            SpmDistribution::Local => (None, None),
            SpmDistribution::Remote => {
                let checksum = xcframework_output
                    .as_ref()
                    .and_then(|output| output.checksum.clone())
                    .map(Ok)
                    .unwrap_or_else(|| {
                        let step = reporter.step("Computing checksum");
                        let result = existing_xcframework_checksum(config);
                        step.finish_success();
                        result
                    })?;
                let version = options
                    .version
                    .or_else(detect_version)
                    .unwrap_or_else(|| "0.1.0".to_string());
                (Some(checksum), Some(version))
            }
        };

        if config.apple_spm_skip_package_swift() {
            reporter.warning("Skipping Package.swift (skip_package_swift = true)");
        } else {
            let generator = match config.apple_spm_distribution() {
                SpmDistribution::Local => SpmPackageGenerator::new_local(config, layout),
                SpmDistribution::Remote => {
                    let checksum = checksum.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires checksum".to_string(),
                        status: None,
                    })?;
                    let version = version.ok_or_else(|| CliError::CommandFailed {
                        command: "remote SPM requires version".to_string(),
                        status: None,
                    })?;
                    SpmPackageGenerator::new_remote(config, checksum, version, layout)
                }
            };

            let step = reporter.step("Generating Package.swift");
            let package_path = generator.generate()?;
            step.finish_success_with(&format!("{}", package_path.display()));
        }
    }

    Ok(())
}

fn build_apple_targets(
    config: &Config,
    targets: &[crate::target::RustTarget],
    release: bool,
    build_cargo_args: &[String],
    selected_crate: &AppleCrateSelection,
    step: &crate::reporter::Step,
) -> Result<()> {
    let on_output: Option<OutputCallback> = if step.is_verbose() {
        Some(Box::new(|line: &str| {
            print_cargo_line(line);
        }))
    } else {
        None
    };

    let build_options = BuildOptions {
        release,
        package: Some(config.library_name().to_string()),
        cargo_args: build_cargo_args.to_vec(),
        env: selected_crate.expansion_env()?,
        on_output,
    };
    let builder = Builder::new(config, build_options);
    let results = builder.build_targets(targets)?;

    if all_successful(&results) {
        return Ok(());
    }

    let failed = failed_targets(&results);
    Err(PackError::BuildFailed { targets: failed }.into())
}

fn generate_apple_bindings(
    config: &Config,
    layout: SpmLayout,
    package_root: &Path,
    header_output_directory: &Path,
    selected_crate: &AppleCrateSelection,
) -> Result<()> {
    let swift_output_directory = match layout {
        SpmLayout::Bundled => config
            .apple_spm_wrapper_sources()
            .map(|path| package_root.join(path).join("BoltFFI"))
            .unwrap_or_else(|| package_root.join("Sources").join("BoltFFI")),
        SpmLayout::FfiOnly => package_root.join("Sources").join("BoltFFI"),
        SpmLayout::Split => config.apple_swift_output().join("BoltFFI"),
    };

    let output = Generation::new(selected_crate.manifest_path.clone())
        .cargo_args(selected_crate.cargo_args.clone())
        .swift_ffi_module(apple_ffi_module_name(config))
        .swift_file(config.swift_bindings_file_stem())
        .swift_custom_mappings(config.apple_swift_custom_mappings())
        .swift_c_header(apple_c_header_path(config))
        .render(BindgenTarget::Swift)
        .map_err(swift_generation_error)?;

    print_coverage(&output);
    write_apple_binding_output(output, &swift_output_directory, header_output_directory)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppleCrateSelection {
    manifest_path: PathBuf,
    source_path: PathBuf,
    cargo_args: Vec<String>,
}

impl AppleCrateSelection {
    fn resolve(config: &Config, build_cargo_args: &[String]) -> Result<Self> {
        let cargo = Cargo::current(build_cargo_args)?;
        let metadata = cargo.metadata()?;
        let cargo_manifest_path = cargo.manifest_path()?;
        let package_selector =
            cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
        let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
        let library_target =
            package.resolve_library_target(&config.crate_artifact_name(), &cargo_manifest_path)?;

        Ok(Self {
            manifest_path: package.manifest_path.clone(),
            source_path: library_target.src_path.clone(),
            cargo_args: cargo.probe_command_arguments(),
        })
    }

    fn expansion_env(&self) -> Result<Vec<(OsString, OsString)>> {
        let root = self
            .manifest_path
            .parent()
            .ok_or_else(|| CliError::CommandFailed {
                command: format!(
                    "manifest path '{}' has no parent directory",
                    self.manifest_path.display()
                ),
                status: None,
            })?;

        Ok(vec![
            (BINDING_EXPANSION_BUILD_ENV.into(), "1".into()),
            (
                BINDING_EXPANSION_ROOT_ENV.into(),
                root.as_os_str().to_owned(),
            ),
            (
                BINDING_EXPANSION_SOURCE_ENV.into(),
                self.source_path.as_os_str().to_owned(),
            ),
            (
                BINDING_EXPANSION_SURFACE_ENV.into(),
                BindingMetadataSurface::Native.as_str().into(),
            ),
            ExpansionRustflags::from_env().into_env(),
        ])
    }
}

enum ExpansionRustflags {
    Encoded(OsString),
    Plain(OsString),
}

impl ExpansionRustflags {
    fn from_env() -> Self {
        std::env::var_os("CARGO_ENCODED_RUSTFLAGS")
            .map(Self::encoded)
            .unwrap_or_else(|| Self::plain(std::env::var_os("RUSTFLAGS")))
    }

    fn encoded(existing: OsString) -> Self {
        Self::Encoded(Self::append_encoded(existing))
    }

    fn plain(existing: Option<OsString>) -> Self {
        Self::Plain(match existing.filter(|value| !value.is_empty()) {
            Some(mut value) => {
                value.push(" --cfg ");
                value.push(BINDING_EXPANSION_CFG);
                value
            }
            None => OsString::from(format!("--cfg {BINDING_EXPANSION_CFG}")),
        })
    }

    fn append_encoded(mut existing: OsString) -> OsString {
        if !existing.is_empty() {
            existing.push(OsStr::new("\u{1f}"));
        }
        existing.push(OsStr::new("--cfg"));
        existing.push(OsStr::new("\u{1f}"));
        existing.push(OsStr::new(BINDING_EXPANSION_CFG));
        existing
    }

    fn into_env(self) -> (OsString, OsString) {
        match self {
            Self::Encoded(value) => ("CARGO_ENCODED_RUSTFLAGS".into(), value),
            Self::Plain(value) => ("RUSTFLAGS".into(), value),
        }
    }
}

fn apple_ffi_module_name(config: &Config) -> String {
    config
        .apple_swift_ffi_module_name()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}FFI", config.xcframework_name()))
}

fn apple_c_header_path(config: &Config) -> PathBuf {
    PathBuf::from(format!("{}.h", config.library_name()))
}

fn write_apple_binding_output(
    output: GeneratedOutput,
    swift_output_directory: &Path,
    header_output_directory: &Path,
) -> Result<()> {
    output.files().iter().try_for_each(|file| {
        let root = if file
            .path()
            .as_path()
            .extension()
            .and_then(|value| value.to_str())
            == Some("h")
        {
            header_output_directory
        } else {
            swift_output_directory
        };
        write_generated_file(&root.join(file.path().as_path()), file.contents())
    })
}

fn write_generated_file(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| CliError::CreateDirectoryFailed {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    fs::write(path, contents).map_err(|source| CliError::WriteFailed {
        path: path.to_path_buf(),
        source,
    })
}

fn print_coverage(output: &GeneratedOutput) {
    let unsupported = output.coverage().unsupported();
    if unsupported.is_empty() {
        return;
    }

    eprintln!("swift generation skipped unsupported declarations");
    eprintln!("{:<12} {:<48} reason", "kind", "name");
    unsupported.iter().for_each(|item| {
        eprintln!(
            "{:<12} {:<48} {}",
            item.declaration().kind(),
            item.declaration().name(),
            item.reason()
        );
    });
}

fn swift_generation_error(error: GenerationError) -> CliError {
    CliError::CommandFailed {
        command: format!("generate swift: {error}"),
        status: None,
    }
}

fn existing_xcframework_checksum(config: &Config) -> Result<String> {
    let xcframework_zip = config
        .apple_xcframework_output()
        .join(format!("{}.xcframework.zip", config.xcframework_name()));

    if xcframework_zip.exists() {
        return compute_checksum(&xcframework_zip);
    }

    Err(CliError::FileNotFound(xcframework_zip))
}

fn detect_version() -> Option<String> {
    std::fs::read_to_string("Cargo.toml")
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("version = "))
                .and_then(|line| {
                    line.split('=')
                        .nth(1)
                        .map(|value| value.trim().trim_matches('"').to_string())
                })
        })
}

fn write_apple_debug_symbols(config: &Config, libraries: &[BuiltLibrary]) -> Result<()> {
    let archive_name = match config.apple_debug_symbols_format() {
        DebugSymbolsFormat::Zip => format!("{}.xcframework.symbols.zip", config.xcframework_name()),
    };
    let bundle = match config.apple_debug_symbols_bundle() {
        DebugSymbolsBundle::Unstripped => "unstripped",
    };
    let artifacts = libraries
        .iter()
        .map(|library| DebugSymbolArtifact {
            source_path: library.path.clone(),
            archive_path: std::path::PathBuf::from(apple_symbol_directory_name(
                library.target.platform(),
            ))
            .join(library.target.triple())
            .join(
                library
                    .path
                    .file_name()
                    .expect("built apple library should have a filename"),
            ),
            kind: DebugSymbolArtifactKind::Static,
            target_triple: Some(library.target.triple().to_string()),
            platform: Some(library.target.platform()),
            architecture: Some(library.target.architecture()),
            abi: None,
            host_target: None,
        })
        .collect::<Vec<_>>();

    write_debug_symbols_zip(
        &config.apple_debug_symbols_output(),
        &archive_name,
        "apple",
        bundle,
        &artifacts,
    )?;

    Ok(())
}

fn apple_symbol_directory_name(platform: Platform) -> &'static str {
    match platform {
        Platform::Ios => "ios",
        Platform::IosSimulator => "ios-simulator",
        Platform::MacOs => "macos",
        Platform::Android | Platform::Wasm | Platform::Linux => unreachable!("non-apple platform"),
    }
}

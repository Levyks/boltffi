use std::path::PathBuf;

use boltffi_backend::{CoverageMode, GeneratedOutput};
use boltffi_bindgen::generate::{Generation, GenerationError};
use boltffi_bindgen::target::Target;

use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::config::Config;

use super::{GenerateOptions, GenerateTarget};

pub fn run_ir_generation(config: &Config, options: &GenerateOptions) -> Result<()> {
    match &options.target {
        GenerateTarget::Python => generate_python(config, options),
        GenerateTarget::KotlinMultiplatform => generate_kmp(config, options),
        other => Err(CliError::CommandFailed {
            command: format!(
                "--ir is only available for python and kmp, not {}",
                target_label(other)
            ),
            status: None,
        }),
    }
}

fn generate_python(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_python_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.python.enabled = false".to_string(),
            status: None,
        });
    }

    let cargo_args = config
        .cargo_args_for_command("generate")
        .into_iter()
        .chain(options.cargo_args.iter().cloned())
        .collect::<Vec<_>>();
    let cargo = Cargo::current(&cargo_args)?;
    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let package_selector =
        cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
    let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
    let library_target =
        package.resolve_library_target(&config.crate_artifact_name(), &cargo_manifest_path)?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.python_output());

    write_python(
        config,
        output_directory,
        package.manifest_path.clone(),
        library_target.name.clone(),
        cargo.probe_command_arguments(),
    )
}

fn generate_kmp(config: &Config, options: &GenerateOptions) -> Result<()> {
    if !config.is_kotlin_multiplatform_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.kotlin_multiplatform.enabled = false".to_string(),
            status: None,
        });
    }

    if !config.should_process(Target::KotlinMultiplatform, options.experimental) {
        return Err(CliError::CommandFailed {
            command: format!(
                "{} is experimental, use --experimental flag or add \"{}\" to [experimental]",
                Target::KotlinMultiplatform.name(),
                Target::KotlinMultiplatform.name()
            ),
            status: None,
        });
    }

    let cargo_args = config
        .cargo_args_for_command("generate")
        .into_iter()
        .chain(options.cargo_args.iter().cloned())
        .collect::<Vec<_>>();
    let cargo = Cargo::current(&cargo_args)?;
    let metadata = cargo.metadata()?;
    let cargo_manifest_path = cargo.manifest_path()?;
    let package_selector =
        cargo.effective_package_selector(config, &metadata, &cargo_manifest_path);
    let package = metadata.find_package(&cargo_manifest_path, package_selector.as_deref())?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.kotlin_multiplatform_output());

    write_kmp(
        output_directory,
        package.manifest_path.clone(),
        cargo.probe_command_arguments(),
    )
}

pub fn run_python_generation(
    config: &Config,
    output: Option<PathBuf>,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
) -> Result<()> {
    if !config.is_python_enabled() {
        return Err(CliError::CommandFailed {
            command: "targets.python.enabled = false".to_string(),
            status: None,
        });
    }

    let output_directory = output.unwrap_or_else(|| config.python_output());

    write_python(
        config,
        output_directory,
        manifest_path,
        artifact_name,
        cargo_args,
    )
}

fn write_python(
    config: &Config,
    output_directory: PathBuf,
    manifest_path: PathBuf,
    artifact_name: String,
    cargo_args: Vec<String>,
) -> Result<()> {
    Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .coverage_mode(CoverageMode::Partial)
        .python_module_name(config.python_module_name())
        .python_distribution_name(config.package.name.clone())
        .python_package_version(config.package_version())
        .python_native_library(artifact_name)
        .render(Target::Python)
        .and_then(|output| {
            print_coverage(&output);
            Generation::write_output(output, &output_directory)
        })
        .map(drop)
        .map_err(|error| generation_error("python", error))
}

fn write_kmp(
    output_directory: PathBuf,
    manifest_path: PathBuf,
    cargo_args: Vec<String>,
) -> Result<()> {
    Generation::new(manifest_path)
        .cargo_args(cargo_args)
        .render(Target::KotlinMultiplatform)
        .and_then(|output| Generation::write_output(output, &output_directory))
        .map(drop)
        .map_err(|error| generation_error("kmp", error))
}

fn print_coverage(output: &GeneratedOutput) {
    let unsupported = output.coverage().unsupported();
    if unsupported.is_empty() {
        return;
    }

    eprintln!("python generation skipped unsupported declarations");
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

fn generation_error(language: &'static str, error: GenerationError) -> CliError {
    CliError::CommandFailed {
        command: format!("generate {language}: {error}"),
        status: None,
    }
}

fn target_label(target: &GenerateTarget) -> &'static str {
    match target {
        GenerateTarget::Swift => "swift",
        GenerateTarget::Kotlin => "kotlin",
        GenerateTarget::KotlinMultiplatform => "kmp",
        GenerateTarget::Java => "java",
        GenerateTarget::Header => "header",
        GenerateTarget::Typescript => "typescript",
        GenerateTarget::Dart => "dart",
        GenerateTarget::Python => "python",
        GenerateTarget::CSharp => "csharp",
        GenerateTarget::All => "all",
    }
}

#[cfg(test)]
mod tests {
    use super::{GenerateOptions, GenerateTarget, run_ir_generation};
    use crate::{cli::CliError, config::Config};

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    #[test]
    fn ir_generation_accepts_kmp_target_before_cargo_probe() {
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = false
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::KotlinMultiplatform,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: Vec::new(),
            },
        )
        .expect_err("disabled KMP IR generation should fail before cargo probing");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command == "targets.kotlin_multiplatform.enabled = false"
        ));
    }

    #[test]
    fn ir_generation_requires_kmp_experimental_opt_in() {
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = true
"#,
        );
        let error = run_ir_generation(
            &config,
            &GenerateOptions {
                target: GenerateTarget::KotlinMultiplatform,
                output: None,
                experimental: false,
                ir: true,
                cargo_args: Vec::new(),
            },
        )
        .expect_err("KMP IR generation should require experimental opt-in");

        assert!(matches!(
            error,
            CliError::CommandFailed { command, status: None }
                if command.contains("kotlin_multiplatform is experimental")
        ));
    }
}

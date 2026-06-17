use boltffi_bindgen::generate::{Generation, GenerationError};
use boltffi_bindgen::target::Target;

use crate::cargo::Cargo;
use crate::cli::{CliError, Result};
use crate::config::Config;

use super::{GenerateOptions, GenerateTarget};

pub fn run_ir_generation(config: &Config, options: &GenerateOptions) -> Result<()> {
    match &options.target {
        GenerateTarget::Python => generate_python(config, options),
        other => Err(CliError::CommandFailed {
            command: format!(
                "--ir is only available for python, not {}",
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

    if !config.should_process(Target::Python, options.experimental) {
        return Err(CliError::CommandFailed {
            command: "python is experimental, use --experimental flag or add \"python\" to [experimental]"
                .to_string(),
            status: None,
        });
    }

    let cargo_args = config
        .cargo_args_for_command("generate")
        .into_iter()
        .chain(options.cargo_args.iter().cloned())
        .collect::<Vec<_>>();
    let manifest_path = Cargo::current(&cargo_args)?.manifest_path()?;
    let output_directory = options
        .output
        .clone()
        .unwrap_or_else(|| config.python_output());

    Generation::new(manifest_path)
        .python_module_name(config.python_module_name())
        .write(Target::Python, &output_directory)
        .map(drop)
        .map_err(generation_error)
}

fn generation_error(error: GenerationError) -> CliError {
    CliError::CommandFailed {
        command: format!("generate python --ir: {error}"),
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

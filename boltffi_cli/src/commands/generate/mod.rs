mod generator;
mod header;
mod languages;

use std::path::{Path, PathBuf};

use boltffi_bindgen::CHeaderLowerer;
use generator::{GenerateRequest, ScanPointerWidth, run_generator};
use header::HeaderGenerator;
use languages::{
    CSharpGenerator, DartGenerator, JavaGenerator, KMPGenerator, KotlinGenerator, PythonGenerator,
    SwiftGenerator, TypeScriptGenerator,
};

use crate::cli::Result;
use crate::config::{Config, Target};

pub enum GenerateTarget {
    Swift,
    Kotlin,
    KotlinMultiplatform,
    Java,
    Header,
    Typescript,
    Dart,
    Python,
    CSharp,
    All,
}

pub struct GenerateOptions {
    pub target: GenerateTarget,
    pub output: Option<PathBuf>,
    pub experimental: bool,
}

pub fn run_generate_with_output(config: &Config, options: GenerateOptions) -> Result<()> {
    let request = GenerateRequest::for_current_crate(config, options.output);

    match options.target {
        GenerateTarget::Swift => run_generator::<SwiftGenerator>(&request, options.experimental),
        GenerateTarget::Kotlin => run_generator::<KotlinGenerator>(&request, options.experimental),
        GenerateTarget::KotlinMultiplatform => {
            run_generator::<KMPGenerator>(&request, options.experimental)
        }
        GenerateTarget::Java => run_generator::<JavaGenerator>(&request, options.experimental),
        GenerateTarget::Header => run_generator::<HeaderGenerator>(&request, options.experimental),
        GenerateTarget::Typescript => {
            run_generator::<TypeScriptGenerator>(&request, options.experimental)
        }
        GenerateTarget::Dart => run_generator::<DartGenerator>(&request, options.experimental),
        GenerateTarget::Python => run_generator::<PythonGenerator>(&request, options.experimental),
        GenerateTarget::CSharp => run_generator::<CSharpGenerator>(&request, options.experimental),
        GenerateTarget::All => {
            if config.should_process(Target::Swift, options.experimental) {
                run_generator::<SwiftGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Kotlin, options.experimental) {
                run_generator::<KotlinGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::KotlinMultiplatform, options.experimental) {
                run_generator::<KMPGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Java, options.experimental) {
                run_generator::<JavaGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::TypeScript, options.experimental) {
                run_generator::<TypeScriptGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Dart, options.experimental) {
                run_generator::<DartGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::Python, options.experimental) {
                run_generator::<PythonGenerator>(&request, options.experimental)?;
            }

            if config.should_process(Target::CSharp, options.experimental) {
                run_generator::<CSharpGenerator>(&request, options.experimental)?;
            }

            Ok(())
        }
    }
}

pub fn run_generate_java_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    JavaGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

pub fn run_generate_kmp_with_output_from_source_dir_and_desktop_fallback_library_name(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
    desktop_fallback_library_name: &str,
) -> Result<()> {
    KMPGenerator::generate_from_source_directory_with_desktop_fallback_library_name(
        config,
        output,
        source_directory,
        crate_name,
        Some(desktop_fallback_library_name),
    )
}

pub fn run_generate_header_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    let output_directory = output
        .as_ref()
        .cloned()
        .unwrap_or_else(|| config.android_header_output());
    let request = GenerateRequest::new(
        config,
        output,
        generator::SourceCrate::new(source_directory, crate_name),
    );

    let output_path = output_directory.join(format!("{}.h", config.library_name()));

    request.ensure_output_directory(&output_directory)?;
    let lowered_crate = request.lowered_crate(ScanPointerWidth::Flexible)?;
    let header_source =
        CHeaderLowerer::new(&lowered_crate.ffi_contract, &lowered_crate.abi_contract).generate();

    request.write_output(&output_path, header_source)
}

pub fn run_generate_python_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    PythonGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

pub fn run_generate_csharp_with_output_from_source_dir(
    config: &Config,
    output: Option<PathBuf>,
    source_directory: &Path,
    crate_name: &str,
) -> Result<()> {
    CSharpGenerator::generate_from_source_directory(config, output, source_directory, crate_name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use boltffi_bindgen::render::python::PythonRuntimeVersion;

    use super::languages::{KMPGenerator, PythonGenerator};
    use crate::config::Config;

    fn parse_config(input: &str) -> Config {
        let parsed: Config = toml::from_str(input).expect("toml parse failed");
        parsed.validate().expect("config validation failed");
        parsed
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        std::env::temp_dir().join(format!("{prefix}-{unique_suffix}"))
    }

    fn demo_source_directory() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/demo")
    }

    #[test]
    fn header_from_source_directory_supports_kmp_only_config() {
        let output_directory = unique_temp_dir("boltffi-kmp-header-generate-test");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"
version = "0.1.0"

[targets.apple]
enabled = false

[targets.android]
enabled = false

[targets.kotlin_multiplatform]
enabled = true
"#,
        );

        super::run_generate_header_with_output_from_source_dir(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
        )
        .expect("kmp-only header generation should succeed");

        let header_path = output_directory.join("demo.h");
        let header = fs::read_to_string(&header_path).expect("header should be readable");

        assert!(header.contains("boltffi"));
        assert!(header.contains("BoltFFICallbackHandle"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }

    #[test]
    fn python_generate_writes_python_package_sources() {
        let output_directory = unique_temp_dir("boltffi-python-generate-test");
        let config = parse_config(
            r#"
[package]
name = "demo"
version = "0.1.0"

[targets.python]
enabled = true
"#,
        );

        PythonGenerator::generate_from_source_directory(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
        )
        .expect("python generate should succeed");

        let generated_init_path = output_directory.join("demo/__init__.py");
        let generated_stub_path = output_directory.join("demo/__init__.pyi");
        let generated_native_path = output_directory.join("demo/_native.c");
        let generated_pyproject_path = output_directory.join("pyproject.toml");
        let generated_setup_path = output_directory.join("setup.py");
        let generated_init = fs::read_to_string(&generated_init_path)
            .expect("generated python init should be readable");
        let generated_stub = fs::read_to_string(&generated_stub_path)
            .expect("generated python typing stub should be readable");
        let generated_native = fs::read_to_string(&generated_native_path)
            .expect("generated native bridge should be readable");
        let generated_pyproject = fs::read_to_string(&generated_pyproject_path)
            .expect("generated pyproject should be readable");
        let generated_setup = fs::read_to_string(&generated_setup_path)
            .expect("generated setup.py should be readable");
        let minimum_python_version_requirement =
            PythonRuntimeVersion::minimum_supported().package_requirement();

        assert!(generated_init.contains("from pathlib import Path"));
        assert!(generated_init.contains("from . import _native"));
        assert!(generated_init.contains("_native._initialize_loader"));
        assert!(generated_init.contains("__all__ = ["));
        assert!(generated_init.contains("PACKAGE_NAME = \"demo\""));
        assert!(generated_stub.contains("MODULE_NAME: str"));
        assert!(generated_stub.contains("def echo_i32"));
        assert!(generated_pyproject.contains("setuptools.build_meta"));
        assert!(generated_setup.contains("Extension("));
        assert!(generated_setup.contains("\"demo._native\""));
        assert!(generated_setup.contains(&format!(
            "python_requires={minimum_python_version_requirement:?}"
        )));
        assert!(generated_native.contains("boltffi_python_symbol_echo_i32_fn"));
        assert!(generated_native.contains("boltffi_python_initialize_loader"));
        assert!(generated_native.contains("PyInit__native"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }

    #[test]
    fn kotlin_multiplatform_generate_writes_kmp_sources() {
        let output_directory = unique_temp_dir("boltffi-kmp-generate-test");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "demo"
version = "0.1.0"

[targets.kotlin_multiplatform]
enabled = true
package = "com.boltffi.demo"

[targets.android.kotlin.type_mappings]
Email = { type = "java.net.URI", conversion = "url_string" }
"#,
        );

        KMPGenerator::generate_from_source_directory_with_desktop_fallback_library_name(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "demo",
            None,
        )
        .expect("kotlin multiplatform generate should succeed");

        let common_path = output_directory.join("src/commonMain/kotlin/com/boltffi/demo/Demo.kt");
        let jvm_actual_path =
            output_directory.join("src/jvmMain/kotlin/com/boltffi/demo/DemoJvmActual.kt");
        let android_actual_path =
            output_directory.join("src/androidMain/kotlin/com/boltffi/demo/DemoAndroidActual.kt");
        let jvm_internal_path =
            output_directory.join("src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt");
        let jni_glue_path = output_directory.join("src/jvmMain/c/jni_glue.c");
        let build_gradle_path = output_directory.join("build.gradle.kts");
        let settings_gradle_path = output_directory.join("settings.gradle.kts");

        let common = fs::read_to_string(&common_path).expect("common source should be readable");
        let jvm_actual =
            fs::read_to_string(&jvm_actual_path).expect("jvm actual should be readable");
        let android_actual =
            fs::read_to_string(&android_actual_path).expect("android actual should be readable");
        let jvm_internal =
            fs::read_to_string(&jvm_internal_path).expect("jvm source should be readable");
        let jni_glue = fs::read_to_string(&jni_glue_path).expect("jni glue should be readable");
        let build_gradle =
            fs::read_to_string(&build_gradle_path).expect("gradle file should be readable");
        let settings_gradle =
            fs::read_to_string(&settings_gradle_path).expect("settings file should be readable");

        assert!(common.contains("package com.boltffi.demo"));
        assert!(common.contains("typealias Email = String"));
        assert!(common.contains(
            "class FfiException(val code: kotlin.Int, message: kotlin.String) : kotlin.Exception(message)"
        ));
        assert!(common.contains("sealed class BoltFFIResult<out T, out E>"));
        assert!(common.contains("data class Point("));
        assert!(common.contains("sealed class MathError : kotlin.Exception()"));
        assert!(common.contains("data class AppError("));
        assert!(common.contains("sealed class ComputeError : kotlin.Exception()"));
        assert!(common.contains("data class Triangle(val a: com.boltffi.demo.Point"));
        assert!(common.contains("data class BenchmarkResponse("));
        assert!(common.contains("val result: BoltFFIResult<DataPoint, ComputeError>"));
        assert!(common.contains("enum class LogLevel(val value: Byte)"));
        assert!(common.contains("expect fun echoBytes"));
        assert!(common.contains("expect fun checkedDivide(a: Int, b: Int): Int"));
        assert!(
            common.contains("expect fun resultToString(v: BoltFFIResult<Int, String>): String")
        );
        assert!(common.contains("Unsupported in the initial KMP generator slice"));
        assert!(jvm_actual.contains("actual fun echoBytes"));
        assert!(jvm_actual.contains("actual fun checkedDivide(a: Int, b: Int): Int"));
        assert!(jvm_actual.contains("catch (err: com.boltffi.demo.jvm.MathError)"));
        assert!(jvm_actual.contains("catch (err: com.boltffi.demo.jvm.FfiException)"));
        assert!(jvm_actual.contains("private fun MathError.toBoltFfiJvm()"));
        assert!(
            jvm_actual.contains("private fun com.boltffi.demo.jvm.MathError.toBoltFfiCommon()")
        );
        assert!(jvm_actual.contains("com.boltffi.demo.jvm.echoBytes"));
        assert!(jvm_actual.contains("toBoltFfiJvm"));
        assert_eq!(jvm_actual, android_actual);
        assert!(jvm_internal.contains("package com.boltffi.demo.jvm"));
        assert!(jvm_internal.contains("typealias Email = String"));
        assert!(jvm_internal.contains("@JvmStatic external fun"));
        assert!(jni_glue.contains("JNIEXPORT"));
        assert!(build_gradle.contains("kotlin(\"multiplatform\")"));
        assert!(build_gradle.contains("kotlin(\"multiplatform\") version \"2.3.21\""));
        assert!(build_gradle.contains("kotlinx-coroutines-core:1.11.0"));
        assert!(build_gradle.contains("import org.jetbrains.kotlin.gradle.dsl.JvmTarget"));
        assert!(build_gradle.contains("jvmTarget.set(JvmTarget.JVM_1_8)"));
        assert!(build_gradle.contains("androidTarget {"));
        assert!(build_gradle.contains("sourceCompatibility = JavaVersion.VERSION_1_8"));
        assert!(build_gradle.contains("targetCompatibility = JavaVersion.VERSION_1_8"));
        assert!(!build_gradle.contains("repositories {"));
        assert!(settings_gradle.contains("pluginManagement"));
        assert!(settings_gradle.contains("gradlePluginPortal()"));
        assert!(settings_gradle.contains("RepositoriesMode.FAIL_ON_PROJECT_REPOS"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }

    #[test]
    fn kotlin_multiplatform_generate_uses_configured_native_load_name() {
        let output_directory = unique_temp_dir("boltffi-kmp-generate-load-name-test");
        let config = parse_config(
            r#"
experimental = ["kotlin_multiplatform"]

[package]
name = "my-lib"
version = "0.1.0"

[targets.android.kotlin]
library_name = "configured-library"

[targets.kotlin_multiplatform]
enabled = true
package = "com.boltffi.demo"
module_name = "Demo"
"#,
        );

        KMPGenerator::generate_from_source_directory_with_desktop_fallback_library_name(
            &config,
            Some(output_directory.clone()),
            &demo_source_directory(),
            "my-lib",
            None,
        )
        .expect("kotlin multiplatform generate should succeed");

        let jvm_internal_path =
            output_directory.join("src/jvmMain/kotlin/com/boltffi/demo/jvm/Demo.kt");
        let jni_glue_path = output_directory.join("src/jvmMain/c/jni_glue.c");
        let jvm_internal =
            fs::read_to_string(&jvm_internal_path).expect("jvm source should be readable");
        let jni_glue = fs::read_to_string(&jni_glue_path).expect("jni glue should be readable");

        assert!(jvm_internal.contains("val androidLibrary = \"configured-library\""));
        assert!(jvm_internal.contains("val desktopPreferredLibrary = \"configured_library_jni\""));
        assert!(jvm_internal.contains("val desktopFallbackLibrary = \"my_lib\""));
        assert!(jni_glue.contains("#include <my-lib.h>"));

        fs::remove_dir_all(output_directory).expect("cleanup generated output");
    }
}

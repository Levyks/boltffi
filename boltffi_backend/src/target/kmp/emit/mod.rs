//! Kotlin Multiplatform file emission from lowered KMP plans.

use std::path::{Path, PathBuf};

use crate::core::{Error, FilePath, GeneratedFile, GeneratedOutput, Result};

use super::plan::{KmpModule, KmpPlatform};

mod common;
mod gradle;
mod jvm;
mod output;

pub use output::{KMP_SUPPORT_REPORT_FILE, KMP_SUPPORT_REPORT_SCHEMA_VERSION};

use output::KmpSupportMetadata;

/// Options that affect KMP output files but not support admission.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpEmissionOptions {
    package_name: String,
    module_name: String,
    min_sdk: u32,
}

impl KmpEmissionOptions {
    /// Creates emission options.
    pub fn new(
        package_name: impl Into<String>,
        module_name: impl Into<String>,
        min_sdk: u32,
    ) -> Self {
        Self {
            package_name: package_name.into(),
            module_name: module_name.into(),
            min_sdk,
        }
    }

    /// Returns the Kotlin package used for common and platform source sets.
    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    /// Returns the Kotlin source/module class name.
    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    /// Returns the Android minSdk written into Gradle output.
    pub const fn min_sdk(&self) -> u32 {
        self.min_sdk
    }
}

/// Emits a lowered KMP module plan into generated files.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpEmitter {
    options: KmpEmissionOptions,
}

impl KmpEmitter {
    /// Creates a KMP emitter from output options.
    pub fn new(options: KmpEmissionOptions) -> Self {
        Self { options }
    }

    /// Emits files for the supplied module plan.
    pub fn emit(&self, module: &KmpModule) -> Result<GeneratedOutput> {
        validate_emission_options(&self.options)?;
        validate_emit_ready(module)?;

        let source_package_path = package_path(self.options.package_name());
        let internal_package = format!("{}.jvm", self.options.package_name());
        let internal_package_path = package_path(&internal_package);
        let common_dir = PathBuf::from("src/commonMain/kotlin").join(&source_package_path);
        let support_metadata = KmpSupportMetadata::new(
            module.support_report(),
            self.options.package_name(),
            self.options.module_name(),
            self.options.min_sdk(),
        );
        let mut support_report =
            serde_json::to_string_pretty(&support_metadata).map_err(|error| Error::Template {
                message: format!("serialize KMP support report: {error}"),
            })?;
        support_report.push('\n');

        let mut files = vec![
            self.file(
                "settings.gradle.kts",
                gradle::render_settings_gradle(self.options.module_name())?,
            )?,
            self.file(
                "build.gradle.kts",
                gradle::render_build_gradle(self.options.package_name(), self.options.min_sdk())?,
            )?,
            self.file(
                common_dir.join(format!("{}.kt", self.options.module_name())),
                common::render_common_module(module, self.options.package_name())?,
            )?,
            self.file(KMP_SUPPORT_REPORT_FILE, support_report)?,
        ];

        for adapter in jvm::default_adapters() {
            let actual_dir = source_set_kotlin_dir(adapter.source_set, &source_package_path);
            files.push(self.file(
                actual_dir.join(format!(
                    "{}{}.kt",
                    self.options.module_name(),
                    adapter.actual_file_suffix
                )),
                jvm::render_platform_actual(self.options.package_name())?,
            )?);
        }

        for adapter in jvm::default_adapters() {
            let internal_dir = source_set_kotlin_dir(adapter.source_set, &internal_package_path);
            files.push(self.file(
                internal_dir.join(format!("{}.kt", self.options.module_name())),
                jvm::render_internal_kotlin(&internal_package)?,
            )?);
        }

        for adapter in jvm::default_adapters() {
            files.push(self.file(
                PathBuf::from(format!("src/{}/c/jni_glue.c", adapter.source_set)),
                jvm::render_jni_glue()?,
            )?);
        }

        Ok(GeneratedOutput::new(files, Vec::new()))
    }

    fn file(&self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Result<GeneratedFile> {
        Ok(GeneratedFile::new(FilePath::new(path)?, contents))
    }
}

fn validate_emission_options(options: &KmpEmissionOptions) -> Result<()> {
    validate_package_name(options.package_name())?;
    validate_module_name(options.module_name())
}

fn validate_package_name(package_name: &str) -> Result<()> {
    for segment in package_name.split('.') {
        validate_relative_path_component(segment)?;
    }

    Ok(())
}

fn validate_module_name(module_name: &str) -> Result<()> {
    validate_relative_path_component(module_name)
}

fn validate_relative_path_component(component: &str) -> Result<()> {
    if component.is_empty()
        || component == "."
        || component == ".."
        || Path::new(component).is_absolute()
        || contains_path_metacharacter(component)
    {
        Err(invalid_emission_options())
    } else {
        Ok(())
    }
}

fn contains_path_metacharacter(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || value.contains(':')
}

fn invalid_emission_options() -> Error {
    Error::UnsupportedTarget {
        target: "kotlin_multiplatform",
        shape: "invalid KMP emission options",
    }
}

fn validate_emit_ready(module: &KmpModule) -> Result<()> {
    if !module.common().apis().is_empty() {
        return Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "KMP declaration body emission",
        });
    }

    let selected = module
        .platforms()
        .iter()
        .map(|platform| platform.platform())
        .collect::<Vec<_>>();
    if selected != KmpPlatform::default_selected() {
        return Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape: "non-default KMP platform emission",
        });
    }

    Ok(())
}

fn package_path(package_name: &str) -> PathBuf {
    package_name.split('.').collect()
}

fn source_set_kotlin_dir(source_set: &str, package_path: &Path) -> PathBuf {
    PathBuf::from(format!("src/{source_set}/kotlin")).join(package_path)
}

#[cfg(test)]
mod tests {
    use super::super::{
        KmpApiPlan, KmpCapability, KmpCapabilitySet, KmpCommonModule, KmpModule, KmpPlatform,
        KmpPlatformModule, KmpSupportApi, KmpSupportMode, KmpSupportReport,
    };
    use super::{KmpEmissionOptions, KmpEmitter};

    fn empty_module() -> KmpModule {
        KmpModule::new(
            KmpCommonModule::new(Vec::new()),
            vec![
                KmpPlatformModule::new(KmpPlatform::Jvm, KmpPlatform::Jvm.capabilities()),
                KmpPlatformModule::new(KmpPlatform::Android, KmpPlatform::Android.capabilities()),
            ],
            KmpSupportReport::new(
                KmpSupportMode::Strict,
                vec![KmpPlatform::Jvm, KmpPlatform::Android],
                Vec::new(),
                vec![KmpSupportApi::rejected(
                    "record method",
                    "point::translate",
                    "mutating receivers on jvm",
                )],
            ),
        )
    }

    fn non_empty_module() -> KmpModule {
        KmpModule::new(
            KmpCommonModule::new(vec![KmpApiPlan::new(
                "function",
                "add",
                KmpCapabilitySet::from_iter([KmpCapability::SyncCallables]),
            )]),
            vec![
                KmpPlatformModule::new(KmpPlatform::Jvm, KmpPlatform::Jvm.capabilities()),
                KmpPlatformModule::new(KmpPlatform::Android, KmpPlatform::Android.capabilities()),
            ],
            KmpSupportReport::new(
                KmpSupportMode::Strict,
                vec![KmpPlatform::Jvm, KmpPlatform::Android],
                vec![KmpSupportApi::admitted("function", "add")],
                Vec::new(),
            ),
        )
    }

    fn jvm_only_module() -> KmpModule {
        KmpModule::new(
            KmpCommonModule::new(Vec::new()),
            vec![KmpPlatformModule::new(
                KmpPlatform::Jvm,
                KmpPlatform::Jvm.capabilities(),
            )],
            KmpSupportReport::new(
                KmpSupportMode::Strict,
                vec![KmpPlatform::Jvm],
                Vec::new(),
                Vec::new(),
            ),
        )
    }

    fn assert_invalid_emission_options(error: crate::Error) {
        assert!(matches!(
            error,
            crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "invalid KMP emission options"
            }
        ));
    }

    #[test]
    fn emitter_rejects_module_names_that_escape_output_root() {
        for module_name in ["/tmp/owned", "../owned", "..", "bad/name", "bad\\name"] {
            let error =
                KmpEmitter::new(KmpEmissionOptions::new("com.example.demo", module_name, 24))
                    .emit(&empty_module())
                    .expect_err("module names must remain a single relative file stem");

            assert_invalid_emission_options(error);
        }
    }

    #[test]
    fn emitter_rejects_package_names_that_escape_output_root() {
        for package_name in [
            "/tmp.owned",
            "../owned",
            "com..demo",
            "com.demo.",
            "com/bad.demo",
            "com\\bad.demo",
        ] {
            let error = KmpEmitter::new(KmpEmissionOptions::new(package_name, "Demo", 24))
                .emit(&empty_module())
                .expect_err("package names must map to relative package path components");

            assert_invalid_emission_options(error);
        }
    }

    #[test]
    fn emitter_rejects_non_empty_common_surface_until_body_emission_is_ported() {
        let error = KmpEmitter::new(KmpEmissionOptions::new("com.example.demo", "Demo", 24))
            .emit(&non_empty_module())
            .expect_err("non-empty KMP common surfaces need body emission before files are safe");

        assert!(matches!(
            error,
            crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "KMP declaration body emission"
            }
        ));
    }

    #[test]
    fn emitter_rejects_non_default_platform_matrix_until_files_are_parameterized() {
        let error = KmpEmitter::new(KmpEmissionOptions::new("com.example.demo", "Demo", 24))
            .emit(&jvm_only_module())
            .expect_err("emitter must not write JVM+Android files for a JVM-only report");

        assert!(matches!(
            error,
            crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "non-default KMP platform emission"
            }
        ));
    }

    #[test]
    fn emitter_uses_legacy_kmp_jvm_android_file_list() {
        let output = KmpEmitter::new(KmpEmissionOptions::new("com.example.demo", "Demo", 24))
            .emit(&empty_module())
            .expect("KMP files should emit");
        let paths = output
            .files()
            .iter()
            .map(|file| file.path().as_path().display().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            paths,
            vec![
                "settings.gradle.kts",
                "build.gradle.kts",
                "src/commonMain/kotlin/com/example/demo/Demo.kt",
                "boltffi-kmp-support.json",
                "src/jvmMain/kotlin/com/example/demo/DemoJvmActual.kt",
                "src/androidMain/kotlin/com/example/demo/DemoAndroidActual.kt",
                "src/jvmMain/kotlin/com/example/demo/jvm/Demo.kt",
                "src/androidMain/kotlin/com/example/demo/jvm/Demo.kt",
                "src/jvmMain/c/jni_glue.c",
                "src/androidMain/c/jni_glue.c",
            ]
        );
    }

    #[test]
    fn emitter_writes_pack_compatible_support_metadata() {
        let output = KmpEmitter::new(KmpEmissionOptions::new("com.example.demo", "Demo", 24))
            .emit(&empty_module())
            .expect("KMP files should emit");
        let report = output
            .files()
            .iter()
            .find(|file| file.path().as_path() == std::path::Path::new("boltffi-kmp-support.json"))
            .expect("support report");
        let json: serde_json::Value =
            serde_json::from_str(report.contents()).expect("valid support metadata");

        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["mode"], "strict");
        assert_eq!(
            json["selected_platforms"],
            serde_json::json!(["jvm", "android"])
        );
        assert_eq!(json["package_name"], "com.example.demo");
        assert_eq!(json["module_name"], "Demo");
        assert_eq!(json["min_sdk"], 24);
        assert_eq!(json["admitted_apis"], serde_json::json!([]));
        assert_eq!(
            json["rejected_apis"][0]["reason"],
            "mutating receivers on jvm"
        );
    }

    #[test]
    fn emitter_keeps_common_runtime_in_common_source() {
        let output = KmpEmitter::new(KmpEmissionOptions::new("com.example.demo", "Demo", 24))
            .emit(&empty_module())
            .expect("KMP files should emit");
        let common = output
            .files()
            .iter()
            .find(|file| {
                file.path().as_path()
                    == std::path::Path::new("src/commonMain/kotlin/com/example/demo/Demo.kt")
            })
            .expect("common source");

        assert!(common.contents().contains("package com.example.demo"));
        assert!(common.contents().contains("class FfiException"));
        assert!(common.contents().contains("sealed class BoltFFIResult"));
    }
}

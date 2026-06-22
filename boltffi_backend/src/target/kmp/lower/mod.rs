//! KMP plan lowering and admission.

pub mod admission;

use std::fmt;

use boltffi_binding::{Bindings, Native};

use super::plan::{
    KmpApiPlan, KmpCommonModule, KmpModule, KmpPlatform, KmpPlatformModule, KmpSupportMode,
    KmpSupportReport,
};

/// Options controlling KMP plan lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpLoweringOptions {
    selected_platforms: Vec<KmpPlatform>,
    support_mode: KmpSupportMode,
}

impl Default for KmpLoweringOptions {
    fn default() -> Self {
        Self {
            selected_platforms: KmpPlatform::default_selected(),
            support_mode: KmpSupportMode::Strict,
        }
    }
}

impl KmpLoweringOptions {
    /// Creates lowering options using the default JVM and Android platform set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the selected KMP platforms.
    pub fn selected_platforms(mut self, selected_platforms: impl Into<Vec<KmpPlatform>>) -> Self {
        self.selected_platforms = selected_platforms.into();
        self
    }

    /// Sets the support mode used for unsupported APIs.
    pub fn support_mode(mut self, support_mode: KmpSupportMode) -> Self {
        self.support_mode = support_mode;
        self
    }

    /// Returns the selected KMP platforms.
    pub fn platforms(&self) -> &[KmpPlatform] {
        &self.selected_platforms
    }

    /// Returns the support mode.
    pub const fn mode(&self) -> KmpSupportMode {
        self.support_mode
    }
}

/// Lowers classified native bindings into a KMP module plan.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct KmpLowerer {
    options: KmpLoweringOptions,
}

impl KmpLowerer {
    /// Creates a lowerer from options.
    pub fn new(options: KmpLoweringOptions) -> Self {
        Self { options }
    }

    /// Lowers bindings into a KMP module plan.
    pub fn lower(
        &self,
        bindings: &Bindings<Native>,
    ) -> std::result::Result<KmpModule, KmpLowerError> {
        let admission = admission::KmpAdmission::for_bindings(
            self.options.selected_platforms.clone(),
            bindings,
        );
        let admission_report = admission.evaluate();
        let admitted = admission_report
            .admitted()
            .iter()
            .map(|record| {
                KmpApiPlan::new(
                    record.kind(),
                    record.name(),
                    record.required_capabilities().clone(),
                )
            })
            .collect::<Vec<_>>();
        let support_report = KmpSupportReport::new(
            self.options.support_mode,
            self.options.selected_platforms.clone(),
            admission_report.admitted_support_apis(),
            admission_report.rejected_support_apis(),
        );

        if self.options.selected_platforms.is_empty() {
            return Err(KmpLowerError::InvalidPlatformMatrix {
                reason: "no selected KMP platforms".to_owned(),
                report: support_report,
            });
        }

        if self.options.support_mode == KmpSupportMode::Strict
            && !support_report.rejected_apis().is_empty()
        {
            return Err(KmpLowerError::UnsupportedApis {
                report: support_report,
            });
        }

        let platforms = self
            .options
            .selected_platforms
            .iter()
            .map(|platform| KmpPlatformModule::new(*platform, platform.capabilities()))
            .collect();

        Ok(KmpModule::new(
            KmpCommonModule::new(admitted),
            platforms,
            support_report,
        ))
    }

    /// Returns the lowerer options.
    pub const fn options(&self) -> &KmpLoweringOptions {
        &self.options
    }
}

/// Failure while lowering a KMP module plan.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum KmpLowerError {
    /// Strict mode rejected at least one API.
    UnsupportedApis {
        /// Full support report describing admitted and rejected APIs.
        report: KmpSupportReport,
    },
    /// The selected platform matrix cannot produce a KMP module.
    InvalidPlatformMatrix {
        /// Human-readable platform matrix failure.
        reason: String,
        /// Support report built with the invalid matrix for diagnostics.
        report: KmpSupportReport,
    },
}

impl KmpLowerError {
    /// Converts this KMP lowering failure into a backend render error.
    pub(crate) fn into_backend_error(self) -> crate::Error {
        match self {
            Self::UnsupportedApis { .. } => crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "unsupported KMP APIs",
            },
            Self::InvalidPlatformMatrix { .. } => crate::Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "invalid KMP platform matrix",
            },
        }
    }
}

impl fmt::Display for KmpLowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedApis { report } => {
                write!(
                    formatter,
                    "unsupported KMP APIs: {}",
                    summarize_report(report)
                )
            }
            Self::InvalidPlatformMatrix { reason, report } => {
                write!(
                    formatter,
                    "invalid KMP platform matrix: {reason}: {}",
                    summarize_report(report)
                )
            }
        }
    }
}

impl std::error::Error for KmpLowerError {}

fn summarize_report(report: &KmpSupportReport) -> String {
    report
        .rejected_apis()
        .iter()
        .take(4)
        .map(|api| match api.reason() {
            Some(reason) => format!("{} {} ({reason})", api.kind(), api.name()),
            None => format!("{} {}", api.kind(), api.name()),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Lowers bindings with default strict JVM and Android options.
pub fn lower(bindings: &Bindings<Native>) -> std::result::Result<KmpModule, KmpLowerError> {
    KmpLowerer::new(KmpLoweringOptions::new()).lower(bindings)
}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower as lower_bindings};

    use super::{
        super::plan::{KmpCapability, KmpPlatform, KmpSupportMode},
        KmpLowerError, KmpLowerer, KmpLoweringOptions,
    };

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source fixture"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower_bindings::<Native>(&source).expect("source should lower")
    }

    fn unsupported_report(error: KmpLowerError) -> super::super::plan::KmpSupportReport {
        match error {
            KmpLowerError::UnsupportedApis { report } => report,
            other => panic!("unexpected KMP lower error: {other:?}"),
        }
    }

    fn admitted_api<'module>(
        module: &'module super::super::plan::KmpModule,
        kind: &str,
        name: &str,
    ) -> &'module super::super::plan::KmpApiPlan {
        module
            .common()
            .apis()
            .iter()
            .find(|api| api.kind() == kind && api.name() == name)
            .expect("admitted API")
    }

    #[test]
    fn lowerer_admits_sync_function_for_default_jvm_android_intersection() {
        let module = super::lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect("sync primitive function should be admitted for JVM and Android");

        assert_eq!(module.platforms().len(), 2);
        assert_eq!(module.platforms()[0].platform(), KmpPlatform::Jvm);
        assert_eq!(module.platforms()[1].platform(), KmpPlatform::Android);
        assert_eq!(module.common().apis().len(), 1);
        assert_eq!(module.common().apis()[0].kind(), "function");
        assert_eq!(module.common().apis()[0].name(), "add");
        assert_eq!(module.support_report().mode(), KmpSupportMode::Strict);
        assert_eq!(module.support_report().admitted_apis().len(), 1);
        assert!(module.support_report().rejected_apis().is_empty());
    }

    #[test]
    fn lowerer_rejects_api_missing_from_any_selected_platform() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new()
                .selected_platforms(vec![KmpPlatform::Jvm, KmpPlatform::IosSimulatorArm64]),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("iOS simulator is not admitted yet");

        let report = unsupported_report(error);
        assert_eq!(
            report.selected_platforms(),
            &[KmpPlatform::Jvm, KmpPlatform::IosSimulatorArm64]
        );
        assert_eq!(report.rejected_apis().len(), 1);
        assert_eq!(report.rejected_apis()[0].kind(), "function");
        assert_eq!(report.rejected_apis()[0].name(), "add");
        assert!(
            report.rejected_apis()[0]
                .reason()
                .expect("rejection reason")
                .contains("synchronous callables on iosSimulatorArm64")
        );
    }

    #[test]
    fn preview_prune_records_rejected_class_surface_without_common_api() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new() -> Self {
                    Engine
                }

                pub fn version(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        let rejected = module.support_report().rejected_apis();
        assert!(module.common().apis().is_empty());
        assert_eq!(
            module.support_report().mode(),
            KmpSupportMode::PreviewPruneUnsupported
        );
        assert!(
            rejected
                .iter()
                .any(|api| api.kind() == "class" && api.name() == "engine")
        );
        assert!(
            rejected
                .iter()
                .any(|api| api.kind() == "class initializer" && api.name() == "engine::new")
        );
        assert!(
            rejected
                .iter()
                .any(|api| api.kind() == "class method" && api.name() == "engine::version")
        );
    }

    #[test]
    fn strict_lowerer_rejects_unsupported_members_on_admitted_parent() {
        let error = super::lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub async fn load(&self) -> i32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("async record methods are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(
            report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "record" && api.name() == "point")
        );
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record method"
                && api.name() == "point::load"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("asynchronous callables on jvm")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_mutating_record_and_enum_methods() {
        let error = super::lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub fn translate(&mut self, dx: i32) {
                    self.x += dx;
                }
            }

            #[data]
            pub enum Mode {
                Fast,
                Slow,
            }

            #[data(impl)]
            impl Mode {
                pub fn flip(&mut self) {
                    *self = Mode::Slow;
                }
            }
            "#,
        ))
        .expect_err("mutating record and enum receivers are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(
            report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "record" && api.name() == "point")
        );
        assert!(
            report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "enum" && api.name() == "mode")
        );
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record method"
                && api.name() == "point::translate"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("mutating receivers on jvm")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "enum method"
                && api.name() == "mode::flip"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("mutating receivers on jvm")
        }));
        assert!(
            !report
                .admitted_apis()
                .iter()
                .any(|api| { matches!(api.name(), "point::translate" | "mode::flip") })
        );
    }

    #[test]
    fn preview_prune_omits_mutating_record_and_enum_methods() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub fn translate(&mut self, dx: i32) {
                    self.x += dx;
                }
            }

            #[data]
            pub enum Mode {
                Fast,
                Slow,
            }

            #[data(impl)]
            impl Mode {
                pub fn flip(&mut self) {
                    *self = Mode::Slow;
                }
            }
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        assert!(
            module
                .common()
                .apis()
                .iter()
                .any(|api| api.kind() == "record" && api.name() == "point")
        );
        assert!(
            module
                .common()
                .apis()
                .iter()
                .any(|api| api.kind() == "enum" && api.name() == "mode")
        );
        assert!(
            !module
                .common()
                .apis()
                .iter()
                .any(|api| { matches!(api.name(), "point::translate" | "mode::flip") })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "record method" && api.name() == "point::translate" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "enum method" && api.name() == "mode::flip" })
        );
    }

    #[test]
    fn lowerer_records_owner_capabilities_on_admitted_members() {
        let module = super::lower(&bindings(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
            }

            #[data(impl)]
            impl Point {
                pub fn stable(&self) -> u32 {
                    1
                }
            }

            #[data]
            pub enum Mode {
                Fast,
                Slow,
            }

            #[data(impl)]
            impl Mode {
                pub fn stable(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect("supported record and enum methods should be admitted");

        let record_method = admitted_api(&module, "record method", "point::stable");
        assert!(
            record_method
                .required_capabilities()
                .contains(KmpCapability::DirectRecords)
        );
        assert!(
            record_method
                .required_capabilities()
                .contains(KmpCapability::SyncCallables)
        );

        let enum_method = admitted_api(&module, "enum method", "mode::stable");
        assert!(
            enum_method
                .required_capabilities()
                .contains(KmpCapability::CStyleEnums)
        );
        assert!(
            enum_method
                .required_capabilities()
                .contains(KmpCapability::SyncCallables)
        );
    }

    #[test]
    fn strict_lowerer_rejects_encoded_record_with_unsupported_field_type() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }
            "#,
        ))
        .expect_err("encoded record callback fields are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(
            report.rejected_apis().iter().any(|api| {
                api.kind() == "record"
                    && api.name() == "bad::record"
                    && api
                        .reason()
                        .expect("rejection reason")
                        .contains("callbacks on jvm")
            }),
            "{:#?}",
            report.rejected_apis()
        );
    }

    #[test]
    fn strict_lowerer_rejects_data_enum_with_unsupported_payload_type() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }
            "#,
        ))
        .expect_err("data enum callback payloads are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(
            report.rejected_apis().iter().any(|api| {
                api.kind() == "enum"
                    && api.name() == "bad::enum"
                    && api
                        .reason()
                        .expect("rejection reason")
                        .contains("callbacks on jvm")
            }),
            "{:#?}",
            report.rejected_apis()
        );
    }

    #[test]
    fn strict_lowerer_rejects_functions_using_rejected_records_and_enums() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub fn use_record(value: BadRecord) -> BadRecord {
                value
            }

            #[export]
            pub fn use_enum(value: BadEnum) -> BadEnum {
                value
            }
            "#,
        ))
        .expect_err("dependent APIs must reject when their record or enum type is rejected");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "use::record"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "use::enum"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(!report.admitted_apis().iter().any(|api| {
            api.kind() == "function" && matches!(api.name(), "use::record" | "use::enum")
        }));
    }

    #[test]
    fn preview_prune_omits_functions_using_rejected_records_and_enums() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub fn use_record(value: BadRecord) -> BadRecord {
                value
            }

            #[export]
            pub fn use_enum(value: BadEnum) -> BadEnum {
                value
            }
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        assert!(!module.common().apis().iter().any(|api| {
            api.kind() == "function" && matches!(api.name(), "use::record" | "use::enum")
        }));
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "function" && api.name() == "use::record" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "function" && api.name() == "use::enum" })
        );
    }

    #[test]
    fn strict_lowerer_cascades_rejected_record_owner_to_members() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub struct BadRecord {
                pub callback: Box<dyn Listener>,
            }

            #[data(impl)]
            impl BadRecord {
                pub fn stable(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("members of rejected records must be rejected too");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "record method"
                && api.name() == "bad::record::stable"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(
            !report.admitted_apis().iter().any(|api| {
                api.kind() == "record method" && api.name() == "bad::record::stable"
            })
        );
    }

    #[test]
    fn strict_lowerer_cascades_rejected_enum_owner_to_members() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                WithCallback(Box<dyn Listener>),
            }

            #[data(impl)]
            impl BadEnum {
                pub fn stable(&self) -> u32 {
                    1
                }
            }
            "#,
        ))
        .expect_err("members of rejected enums must be rejected too");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "enum method"
                && api.name() == "bad::enum::stable"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("callbacks on jvm")
        }));
        assert!(
            !report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "enum method" && api.name() == "bad::enum::stable")
        );
    }

    #[test]
    fn strict_lowerer_rejects_empty_platform_matrix() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new().selected_platforms(Vec::<KmpPlatform>::new()),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        ))
        .expect_err("KMP planning needs at least one selected platform");

        let KmpLowerError::InvalidPlatformMatrix { reason, report } = error else {
            panic!("unexpected KMP lower error: {error:?}");
        };
        assert!(reason.contains("no selected KMP platforms"));
        assert_eq!(report.selected_platforms(), &[]);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "add"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("no selected KMP platforms")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_empty_platform_matrix_without_apis() {
        let error = KmpLowerer::new(
            KmpLoweringOptions::new().selected_platforms(Vec::<KmpPlatform>::new()),
        )
        .lower(&bindings(""))
        .expect_err("KMP planning needs at least one selected platform even for empty bindings");

        let KmpLowerError::InvalidPlatformMatrix { reason, report } = error else {
            panic!("unexpected KMP lower error: {error:?}");
        };
        assert!(reason.contains("no selected KMP platforms"));
        assert_eq!(report.selected_platforms(), &[]);
        assert!(report.rejected_apis().is_empty());
    }

    #[test]
    fn strict_lowerer_rejects_inline_constant_using_rejected_enum() {
        let error = super::lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                Good,
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub const DEFAULT_BAD: BadEnum = BadEnum::Good;
            "#,
        ))
        .expect_err("inline constants must reject when their declared type is rejected");

        let KmpLowerError::UnsupportedApis { report } = error else {
            panic!("unexpected KMP lower error: {error:?}");
        };
        assert!(
            report.rejected_apis().iter().any(|api| {
                api.kind() == "constant"
                    && api.name() == "default::bad"
                    && api
                        .reason()
                        .expect("rejection reason")
                        .contains("callbacks on jvm")
            }),
            "{:#?}",
            report.rejected_apis()
        );
        assert!(
            !report
                .admitted_apis()
                .iter()
                .any(|api| api.kind() == "constant" && api.name() == "default::bad")
        );
    }

    #[test]
    fn preview_prune_omits_inline_constant_using_rejected_enum() {
        let module = KmpLowerer::new(
            KmpLoweringOptions::new().support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(&bindings(
            r#"
            #[export]
            pub trait Listener {
                fn notify(&self);
            }

            #[data]
            pub enum BadEnum {
                Good,
                WithCallback(Box<dyn Listener>),
            }

            #[export]
            pub const DEFAULT_BAD: BadEnum = BadEnum::Good;
            "#,
        ))
        .expect("preview mode should return a pruned plan");

        assert!(
            !module
                .common()
                .apis()
                .iter()
                .any(|api| { api.kind() == "constant" && api.name() == "default::bad" })
        );
        assert!(
            module
                .support_report()
                .rejected_apis()
                .iter()
                .any(|api| { api.kind() == "constant" && api.name() == "default::bad" }),
            "{:#?}",
            module.support_report().rejected_apis()
        );
    }

    #[test]
    fn strict_lowerer_rejects_unsupported_fallible_error_payload() {
        let error = super::lower(&bindings(
            r#"
            use std::time::Duration;

            #[export]
            pub fn load() -> Result<String, Duration> {
                Ok(String::new())
            }
            "#,
        ))
        .expect_err("builtin error payloads are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "load"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_unmodeled_typeref_families() {
        let error = super::lower(&bindings(
            r#"
            use std::collections::HashMap;

            #[export]
            pub fn echo_map(value: HashMap<String, String>) -> HashMap<String, String> {
                value
            }
            "#,
        ))
        .expect_err("maps are outside the M1b KMP capability set");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "echo::map"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
    }

    #[test]
    fn strict_lowerer_rejects_custom_type_with_unsupported_representation() {
        let error = super::lower(&bindings(
            r#"
            use std::time::Duration;

            custom_type!(
                BadDuration,
                remote = RemoteDuration,
                repr = Duration,
                into_ffi = |_value: &RemoteDuration| Duration::from_secs(0),
                try_from_ffi = |_value: Duration| Ok(RemoteDuration),
            );

            #[export]
            pub fn echo_bad(value: RemoteDuration) -> RemoteDuration {
                value
            }
            "#,
        ))
        .expect_err("custom type representations must be admitted too");

        let report = unsupported_report(error);
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "custom type"
                && api.name() == "bad::duration"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
        assert!(report.rejected_apis().iter().any(|api| {
            api.kind() == "function"
                && api.name() == "echo::bad"
                && api
                    .reason()
                    .expect("rejection reason")
                    .contains("unknown binding shapes on jvm")
        }));
    }
}

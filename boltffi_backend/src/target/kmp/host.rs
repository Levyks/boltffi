use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::core::{
    BindingCapability, BridgeCapability, CapabilityRequirements, DeclarationLabel, Diagnostic,
    Emitted, GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result, Target,
    contract::sealed, host,
};

use super::{
    KmpBridge, KmpBridgeContract, KmpPlatform, KmpSupportMode, Syntax,
    lower::{KmpLowerer, KmpLoweringOptions, admission::KmpAdmission},
};

/// Kotlin Multiplatform host renderer for the IR backend plan.
///
/// The host currently lowers to a typed [`super::KmpModule`] plan and emits no
/// Kotlin strings. Complete coverage rendering remains strict: APIs outside
/// the selected platform capability intersection produce diagnostics that the
/// backend driver turns into generation failures.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpHost {
    selected_platforms: Vec<KmpPlatform>,
}

impl KmpHost {
    /// Creates a KMP host renderer.
    pub fn new() -> Self {
        Self {
            selected_platforms: KmpPlatform::default_selected(),
        }
    }

    /// Selects the KMP platform matrix checked by admission.
    pub fn selected_platforms(mut self, platforms: impl Into<Vec<KmpPlatform>>) -> Self {
        self.selected_platforms = platforms.into();
        self
    }

    /// Creates the backend target stack for this skeletal KMP host.
    pub fn into_target(self) -> Target<Self, KmpBridge> {
        Target::new(self, KmpBridge)
    }

    fn emit_admitted(
        &self,
        declaration: boltffi_binding::DeclarationRef<'_, Native>,
        bindings: &Bindings<Native>,
    ) -> Emitted {
        let label = DeclarationLabel::from_ref(declaration);
        let records = KmpAdmission::for_bindings(self.selected_platforms.clone(), bindings)
            .evaluate_declaration(declaration);
        let diagnostics = records
            .iter()
            .filter(|record| !record.is_admitted())
            .map(|record| Diagnostic::new(admission_message(&label, record)))
            .collect::<Vec<_>>();
        if diagnostics.is_empty() {
            Emitted::primary("")
        } else {
            Emitted::primary("").with_diagnostics(diagnostics)
        }
    }
}

fn admission_message(
    label: &DeclarationLabel,
    record: &super::lower::admission::KmpAdmissionRecord,
) -> String {
    let reason = record.reason().unwrap_or("unsupported");
    if record.kind() == label.kind() && record.name() == label.name() {
        reason.to_owned()
    } else {
        format!("{} {}: {reason}", record.kind(), record.name())
    }
}

impl Default for KmpHost {
    fn default() -> Self {
        Self::new()
    }
}

impl host::HostBackend for KmpHost {
    type Surface = Native;
    type Bridge = KmpBridgeContract;
    type Syntax = Syntax;

    fn name(&self) -> &'static str {
        "kotlin_multiplatform"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
            .stable(BindingCapability::Streams)
            .stable(BindingCapability::Constants)
            .stable(BindingCapability::CustomTypes)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new()
    }

    fn record(
        &self,
        decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Record(decl),
            context.bindings(),
        ))
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Enum(decl),
            context.bindings(),
        ))
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Function(decl),
            context.bindings(),
        ))
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Class(decl),
            context.bindings(),
        ))
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Callback(decl),
            context.bindings(),
        ))
    }

    fn stream(
        &self,
        decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Stream(decl),
            context.bindings(),
        ))
    }

    fn constant(
        &self,
        decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::Constant(decl),
            context.bindings(),
        ))
    }

    fn custom_type(
        &self,
        decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Ok(self.emit_admitted(
            boltffi_binding::DeclarationRef::CustomType(decl),
            context.bindings(),
        ))
    }

    fn assemble<'decl>(
        &self,
        bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        KmpLowerer::new(
            KmpLoweringOptions::new()
                .selected_platforms(self.selected_platforms.clone())
                .support_mode(KmpSupportMode::PreviewPruneUnsupported),
        )
        .lower(bindings)
        .map_err(|error| error.into_backend_error())?;
        Ok(GeneratedOutput::new(
            Vec::new(),
            declarations
                .iter()
                .flat_map(|declaration| declaration.emitted().diagnostics().iter().cloned())
                .collect(),
        ))
    }
}

impl sealed::HostBackend for KmpHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        Error,
        target::kmp::{KmpHost, KmpPlatform},
    };

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source fixture"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    #[test]
    fn kmp_target_renders_empty_surface_without_files() {
        let output = KmpHost::new()
            .into_target()
            .render(&bindings(""))
            .expect("empty KMP IR skeleton should render");

        assert!(output.files().is_empty());
        assert!(output.diagnostics().is_empty());
        assert!(output.coverage().is_complete());
    }

    #[test]
    fn kmp_target_renders_admitted_sync_function_without_files() {
        let output = KmpHost::new()
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("KMP IR plan should admit sync primitive functions for JVM and Android");

        assert!(output.files().is_empty());
        assert!(output.diagnostics().is_empty());
        assert!(output.coverage().is_complete());
    }

    #[test]
    fn kmp_target_rejects_apis_outside_platform_intersection() {
        let error = KmpHost::new()
            .selected_platforms(vec![KmpPlatform::Jvm, KmpPlatform::IosSimulatorArm64])
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect_err("KMP IR plan should reject APIs not supported by every platform");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function add"));
                assert!(reason.contains("synchronous callables on iosSimulatorArm64"));
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_empty_platform_matrix() {
        let error = KmpHost::new()
            .selected_platforms(Vec::<KmpPlatform>::new())
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect_err("KMP IR plan should require at least one selected platform");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function add"), "{reason}");
                assert!(reason.contains("no selected KMP platforms"), "{reason}");
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_empty_platform_matrix_without_apis() {
        let error = KmpHost::new()
            .selected_platforms(Vec::<KmpPlatform>::new())
            .into_target()
            .render(&bindings(""))
            .expect_err("KMP IR plan should require at least one selected platform");

        match error {
            Error::UnsupportedTarget {
                target: "kotlin_multiplatform",
                shape: "invalid KMP platform matrix",
            } => {}
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_rejects_api_using_custom_type_with_unsupported_representation() {
        let error = KmpHost::new()
            .into_target()
            .render(&bindings(
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
            .expect_err("KMP IR plan should reject custom type representations it cannot admit");

        match error {
            Error::IncompleteCoverage {
                target: "kotlin_multiplatform",
                reason,
            } => {
                assert!(reason.contains("function echo::bad"), "{reason}");
                assert!(reason.contains("unknown binding shapes on jvm"), "{reason}");
            }
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_reports_unsupported_apis_in_partial_mode() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
                r#"
                pub struct Engine;

                #[export(single_threaded)]
                impl Engine {
                    pub fn new() -> Self {
                        Engine
                    }
                }
                "#,
            ))
            .expect("partial KMP IR skeleton should report unsupported APIs");
        let unsupported = output.coverage().unsupported();

        assert!(output.files().is_empty());
        assert_eq!(output.diagnostics().len(), 2);
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message()
                    == "unsupported classes on jvm, classes on android")
        );
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.message()
                    == "class initializer engine::new: unsupported classes on jvm, classes on android")
        );
        assert_eq!(unsupported.len(), 2);
        assert_eq!(unsupported[0].declaration().kind(), "class");
        assert_eq!(unsupported[0].declaration().name(), "engine");
        assert_eq!(
            unsupported[0].reason(),
            "unsupported classes on jvm, classes on android"
        );
    }

    #[test]
    fn kmp_target_reports_every_unsupported_owned_api_in_partial_mode() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
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

                    pub async fn save(&self) -> i32 {
                        2
                    }
                }
                "#,
            ))
            .expect("partial KMP IR skeleton should report every unsupported owned API");
        let unsupported = output.coverage().unsupported();
        let diagnostics = output.diagnostics();

        assert!(output.files().is_empty());
        assert_eq!(diagnostics.len(), 2);
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("record method point::load"))
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message().contains("record method point::save"))
        );
        assert_eq!(unsupported.len(), 2);
    }
}

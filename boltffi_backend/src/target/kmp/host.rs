use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::core::{
    BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error, GeneratedOutput,
    HostCapabilities, RenderContext, RenderedDeclaration, Result, Target, contract::sealed, host,
};

use super::{KmpBridge, KmpBridgeContract, Syntax};

const M1A_ADMISSION_REASON: &str =
    "KMP IR backend skeleton rejects exported APIs until M1b admission is implemented";

/// Kotlin Multiplatform host renderer for the IR backend skeleton.
///
/// M1a wires KMP into the typed backend pipeline but deliberately does not
/// admit any exported API. Complete coverage generation therefore fails for
/// non-empty binding surfaces, preserving the strict KMP contract until the
/// M1b plan/lower/admission layer can decide the supported common surface.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct KmpHost;

impl KmpHost {
    /// Creates a KMP host renderer.
    pub const fn new() -> Self {
        Self
    }

    /// Creates the backend target stack for this skeletal KMP host.
    pub const fn into_target(self) -> Target<Self, KmpBridge> {
        Target::new(self, KmpBridge)
    }

    fn unsupported(&self, shape: &'static str) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: "kotlin_multiplatform",
            shape,
        })
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
            .in_progress(BindingCapability::Records, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::Enums, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::Functions, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::Classes, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::Callbacks, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::Streams, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::Constants, M1A_ADMISSION_REASON)
            .in_progress(BindingCapability::CustomTypes, M1A_ADMISSION_REASON)
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new()
    }

    fn record(
        &self,
        _decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("record declarations")
    }

    fn enumeration(
        &self,
        _decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("enum declarations")
    }

    fn function(
        &self,
        _decl: &FunctionDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("function declarations")
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("class declarations")
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("callback declarations")
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("stream declarations")
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("constant declarations")
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        self.unsupported("custom type declarations")
    }

    fn assemble<'decl>(
        &self,
        _bindings: &Bindings<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        _declarations: Vec<RenderedDeclaration<'decl, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        Ok(GeneratedOutput::empty())
    }
}

impl sealed::HostBackend for KmpHost {}

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        Error,
        core::{BindingCapability, CapabilityStatus},
        target::kmp::KmpHost,
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
    fn kmp_target_rejects_exported_apis_in_complete_mode() {
        let error = KmpHost::new()
            .into_target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect_err("KMP IR skeleton should reject exported APIs");

        match error {
            Error::BindingCapability {
                target: "kotlin_multiplatform",
                capability: BindingCapability::Functions,
                status: CapabilityStatus::InProgress { reason },
            } => assert_eq!(reason, super::M1A_ADMISSION_REASON),
            other => panic!("unexpected KMP IR skeleton error: {other:?}"),
        }
    }

    #[test]
    fn kmp_target_reports_unsupported_apis_in_partial_mode() {
        let output = KmpHost::new()
            .into_target()
            .render_partial(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("partial KMP IR skeleton should report unsupported APIs");
        let unsupported = output.coverage().unsupported();

        assert!(output.files().is_empty());
        assert_eq!(unsupported.len(), 1);
        assert_eq!(unsupported[0].declaration().kind(), "function");
        assert_eq!(unsupported[0].declaration().name(), "add");
        assert_eq!(unsupported[0].reason(), super::M1A_ADMISSION_REASON);
    }
}

use boltffi_binding::DeclarationId;
use thiserror::Error;

use crate::{BindingCapability, BridgeCapability, CapabilityStatus};

/// Result type used by backend rendering.
pub type Result<T> = std::result::Result<T, Error>;

/// Failure while composing or rendering a backend target.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// A host cannot render a binding declaration shape present in the contract.
    #[error("backend `{target}` does not support binding capability {capability:?}: {status:?}")]
    BindingCapability {
        /// Backend target name.
        target: &'static str,
        /// Required binding capability.
        capability: BindingCapability,
        /// Support status advertised by the host.
        status: CapabilityStatus,
    },
    /// A host requires a bridge capability the selected bridge stack does not provide.
    #[error("backend `{target}` requires bridge capability {capability:?}: {status:?}")]
    BridgeCapability {
        /// Backend target name.
        target: &'static str,
        /// Required bridge capability.
        capability: BridgeCapability,
        /// Support status advertised by the bridge contract.
        status: CapabilityStatus,
    },
    /// A declaration variant reached a backend that has no renderer for it.
    #[error("backend `{target}` cannot render declaration {declaration:?}")]
    UnsupportedDeclaration {
        /// Backend target name.
        target: &'static str,
        /// Declaration identity.
        declaration: DeclarationId,
    },
    /// A generated file path was empty.
    #[error("generated file path cannot be empty")]
    EmptyFilePath,
}

use thiserror::Error as ThisError;

use crate::core::{BindingCapability, BridgeCapability, CapabilityStatus};

/// Result type used by backend rendering.
pub type Result<T> = std::result::Result<T, BackendError>;

/// Short name for backend failures.
pub type Error = BackendError;

/// Failure while composing or rendering a backend target.
#[derive(Clone, Debug, Eq, ThisError, PartialEq)]
#[non_exhaustive]
pub enum BackendError {
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
    /// A generated file path was empty.
    #[error("generated file path cannot be empty")]
    EmptyFilePath,
    /// Anonymous output was assembled with a layout that does not name exactly one file.
    #[error("anonymous emitted output requires a single-file layout")]
    AnonymousOutputNeedsSingleFile,
    /// A rendered declaration did not match any generated file plan.
    #[error("no generated file plan matched {declaration}")]
    UnmatchedFilePlan {
        /// Declaration kind that had no matching file plan.
        declaration: &'static str,
    },
    /// The C ABI bridge cannot render the supplied binding shape.
    #[error("C ABI bridge cannot render {shape}")]
    UnsupportedCAbi {
        /// Binding shape that has no C ABI rendering.
        shape: &'static str,
    },
    /// A host target cannot render the supplied binding shape.
    #[error("{target} target cannot render {shape}")]
    UnsupportedTarget {
        /// Host target name.
        target: &'static str,
        /// Binding shape that has no target rendering.
        shape: &'static str,
    },
    /// A generated C identifier was invalid.
    #[error("invalid C identifier `{identifier}`")]
    InvalidCIdentifier {
        /// Invalid identifier text.
        identifier: String,
    },
    /// A generated C include path was invalid.
    #[error("invalid C include path `{path}`")]
    InvalidCIncludePath {
        /// Invalid include path text.
        path: String,
    },
    /// A generated CPython method name was invalid.
    #[error("invalid CPython method name `{name}`")]
    InvalidPythonMethodName {
        /// Invalid method name text.
        name: String,
    },
    /// A generated Python package module name was invalid.
    #[error("invalid Python package module name `{name}`")]
    InvalidPythonPackageModule {
        /// Invalid module name text.
        name: String,
    },
    /// Two generated Python declarations require the same name in one scope.
    #[error("python name collision in {scope}: `{name}` is used by {existing} and {colliding}")]
    PythonNameCollision {
        /// Python scope where the collision was found.
        scope: String,
        /// Generated Python name used more than once.
        name: String,
        /// Declaration that claimed the generated name first.
        existing: String,
        /// Declaration that collided with the existing name.
        colliding: String,
    },
    /// A backend template failed to render.
    #[error("template rendering failed: {message}")]
    Template {
        /// Template rendering error message.
        message: String,
    },
}

impl From<askama::Error> for BackendError {
    fn from(error: askama::Error) -> Self {
        Self::Template {
            message: error.to_string(),
        }
    }
}

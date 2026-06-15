//! Backend rendering contracts for classified BoltFFI bindings.
//!
//! `boltffi_backend` is the layer below `boltffi_bindgen` and above
//! generated target-language files. It accepts validated
//! [`boltffi_binding::Bindings`] values and exposes typed composition for
//! bridge layers and host-language renderers.
//!
//! Bridge renderers produce the ABI surface a host consumes. Host
//! renderers produce the target-language API over that bridge. A
//! [`Target`] ties both together with an associated-type constraint, so a
//! host cannot be paired with a bridge stack whose contract it does not
//! accept.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bridge;
mod capability;
mod context;
mod error;
pub mod host;
mod output;
mod sealed;
mod target;

pub use capability::{
    BindingCapability, BridgeCapability, CapabilityRequirements, CapabilitySet, CapabilityStatus,
};
pub use context::RenderContext;
pub use error::{Error, Result};
pub use output::{Diagnostic, Emitted, File, FilePath, Fragment};
pub use target::Target;

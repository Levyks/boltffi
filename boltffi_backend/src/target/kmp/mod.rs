//! Kotlin Multiplatform target skeleton for the IR backend pipeline.
//!
//! This module intentionally owns only the new backend boundary in M1a. It
//! does not render the production JVM/Android KMP output yet; that remains in
//! the legacy bindgen renderer until the later KMP planning and parity
//! milestones move behavior into this crate.

mod bridge;
mod host;
mod syntax;

pub use bridge::{KmpBridge, KmpBridgeContract};
pub use host::KmpHost;
pub use syntax::Syntax;

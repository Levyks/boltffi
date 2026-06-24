//! Kotlin Multiplatform target skeleton for the IR backend pipeline.
//!
//! This module intentionally owns only the new backend boundary in M1a. It
//! does not render the production JVM/Android KMP output yet; that remains in
//! the legacy bindgen renderer until the later KMP planning and parity
//! milestones move behavior into this crate.

mod bridge;
pub mod emit;
mod host;
pub mod lower;
pub mod plan;
mod syntax;

pub use bridge::{KmpBridge, KmpBridgeContract};
pub use emit::{KMP_SUPPORT_REPORT_FILE, KmpEmissionOptions, KmpEmitter};
pub use host::KmpHost;
pub use lower::{KmpLowerError, KmpLowerer, KmpLoweringOptions};
pub use plan::{
    KmpApiPlan, KmpCapability, KmpCapabilitySet, KmpCommonModule, KmpModule, KmpPlatform,
    KmpPlatformModule, KmpSupportApi, KmpSupportMode, KmpSupportReport,
};
pub use syntax::Syntax;

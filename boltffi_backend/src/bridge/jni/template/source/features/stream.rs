//! Feature scan for stream template fragments.
//!
//! Stream protocols add direct-batch native methods when their item layout can
//! be copied as a Java byte array. This module records whether those fragments
//! are needed.

use crate::bridge::jni::template::stream::DirectStreamBatchView;

pub struct StreamFeatures {
    pub returns_direct_batches: bool,
}

impl StreamFeatures {
    pub fn from_direct_batches(direct_batches: &[DirectStreamBatchView]) -> Self {
        Self {
            returns_direct_batches: !direct_batches.is_empty(),
        }
    }
}

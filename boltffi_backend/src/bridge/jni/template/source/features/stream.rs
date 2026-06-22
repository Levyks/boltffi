use super::DirectStreamBatchView;

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

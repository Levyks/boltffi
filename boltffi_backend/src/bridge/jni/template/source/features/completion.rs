use super::CallbackCompletionInvokerView;

pub struct CompletionFeatures {
    pub uses_byte_arrays: bool,
    pub uses_record_arrays: bool,
}

impl CompletionFeatures {
    pub fn from_invokers(completions: &[CallbackCompletionInvokerView]) -> Self {
        Self {
            uses_byte_arrays: completions
                .iter()
                .any(|completion| completion.payload_bytes || completion.payload_record),
            uses_record_arrays: completions
                .iter()
                .any(|completion| completion.payload_record),
        }
    }
}

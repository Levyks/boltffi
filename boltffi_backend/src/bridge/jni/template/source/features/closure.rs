use crate::bridge::jni::template::closure::ClosureRegistrationView;

pub struct ClosureFeatures {
    pub has_registrations: bool,
    pub uses_byte_arrays: bool,
    pub uses_direct_vectors: bool,
    pub returns_byte_arrays: bool,
    pub returns_records: bool,
    pub returns_callback_handles: bool,
}

impl ClosureFeatures {
    pub fn from_registrations(closures: &[ClosureRegistrationView]) -> Self {
        Self {
            has_registrations: !closures.is_empty(),
            uses_byte_arrays: closures.iter().any(|closure| {
                !closure.byte_arrays.is_empty() || !closure.handle_byte_arrays.is_empty()
            }),
            uses_direct_vectors: closures.iter().any(|closure| {
                !closure.direct_vectors.is_empty() || !closure.handle_direct_vectors.is_empty()
            }),
            returns_byte_arrays: closures
                .iter()
                .any(|closure| closure.returns_bytes || closure.returns_record),
            returns_records: closures.iter().any(|closure| closure.returns_record),
            returns_callback_handles: closures
                .iter()
                .any(|closure| closure.returns_callback_handle),
        }
    }
}

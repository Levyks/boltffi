use super::CallbackRegistrationView;

pub struct CallbackFeatures {
    pub has_registrations: bool,
    pub uses_byte_arrays: bool,
    pub uses_direct_vectors: bool,
    pub uses_record_arrays: bool,
    pub uses_handles: bool,
    pub returns_byte_arrays: bool,
    pub returns_records: bool,
    pub returns_callback_handles: bool,
}

impl CallbackFeatures {
    pub fn from_registrations(callbacks: &[CallbackRegistrationView]) -> Self {
        Self {
            has_registrations: !callbacks.is_empty(),
            uses_byte_arrays: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.byte_arrays.is_empty())
            }),
            uses_direct_vectors: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.direct_vectors.is_empty())
            }),
            uses_record_arrays: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.record_arrays.is_empty())
            }),
            uses_handles: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| !method.callback_handles.is_empty())
            }),
            returns_byte_arrays: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| method.returns_bytes || method.returns_record)
            }),
            returns_records: callbacks
                .iter()
                .any(|callback| callback.methods.iter().any(|method| method.returns_record)),
            returns_callback_handles: callbacks.iter().any(|callback| {
                callback
                    .methods
                    .iter()
                    .any(|method| method.returns_callback_handle)
            }),
        }
    }
}

use crate::bridge::jni::template::method::NativeMethodView;

pub struct MethodFeatures {
    pub checks_status: bool,
    pub uses_continuations: bool,
    pub returns_byte_arrays: bool,
    pub uses_record_arrays: bool,
    pub uses_exceptions: bool,
    pub returns_callback_handles: bool,
}

impl MethodFeatures {
    pub fn from_methods(methods: &[NativeMethodView]) -> Self {
        Self {
            checks_status: methods.iter().any(|method| method.checks_status),
            uses_continuations: methods.iter().any(|method| method.uses_continuations),
            returns_byte_arrays: methods.iter().any(|method| method.returns_bytes),
            uses_record_arrays: methods
                .iter()
                .any(|method| method.returns_record || !method.record_arrays.is_empty()),
            uses_exceptions: methods.iter().any(|method| {
                method.checks_status
                    || method.returns_bytes
                    || method.returns_record
                    || method.returns_callback
                    || !method.borrowed_arrays.is_empty()
                    || !method.record_arrays.is_empty()
            }),
            returns_callback_handles: methods.iter().any(|method| method.returns_callback),
        }
    }
}

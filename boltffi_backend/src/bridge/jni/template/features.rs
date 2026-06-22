use super::{
    callback::CallbackRegistrationView,
    closure::{CallbackClosureHandleView, ClosureRegistrationView},
    method::NativeMethodView,
};

pub struct SourceFeatures {
    pub uses_limits: bool,
    pub checks_status: bool,
    pub uses_byte_arrays: bool,
    pub uses_record_arrays: bool,
    pub uses_exceptions: bool,
    pub uses_lifecycle: bool,
    pub uses_continuations: bool,
    pub uses_callback_handles: bool,
    pub uses_closure_handles: bool,
}

impl SourceFeatures {
    pub fn from_views(
        methods: &[NativeMethodView],
        callbacks: &[CallbackRegistrationView],
        closures: &[ClosureRegistrationView],
        closure_handles: &[CallbackClosureHandleView],
    ) -> Self {
        let callback_byte_arrays = Self::callback_byte_arrays(callbacks);
        let callback_direct_vectors = Self::callback_direct_vectors(callbacks);
        let callback_record_arrays = Self::callback_record_arrays(callbacks);
        let callback_handles = Self::callback_handles(callbacks);
        let uses_closure_handles = !closure_handles.is_empty();
        let byte_array_returns = Self::byte_array_returns(callbacks, closures);
        let record_returns = Self::record_returns(callbacks, closures);
        let method_byte_array_returns = methods.iter().any(|method| method.returns_bytes);
        let method_record_arrays = methods
            .iter()
            .any(|method| method.returns_record || !method.record_arrays.is_empty());
        let method_exceptions = methods.iter().any(|method| {
            method.checks_status
                || method.returns_bytes
                || method.returns_record
                || method.returns_callback
                || !method.borrowed_arrays.is_empty()
                || !method.record_arrays.is_empty()
        });
        let uses_continuations = methods.iter().any(|method| method.uses_continuations);
        let uses_byte_arrays =
            callback_byte_arrays || byte_array_returns || method_byte_array_returns;
        let uses_record_arrays = method_record_arrays || callback_record_arrays || record_returns;

        Self {
            uses_limits: uses_byte_arrays || uses_record_arrays || callback_direct_vectors,
            checks_status: methods.iter().any(|method| method.checks_status),
            uses_byte_arrays,
            uses_record_arrays,
            uses_exceptions: callback_byte_arrays
                || callback_direct_vectors
                || callback_record_arrays
                || callback_handles
                || uses_closure_handles
                || byte_array_returns
                || method_exceptions,
            uses_continuations,
            uses_lifecycle: uses_continuations || !callbacks.is_empty() || !closures.is_empty(),
            uses_callback_handles: callback_handles
                || methods.iter().any(|method| method.returns_callback),
            uses_closure_handles,
        }
    }

    fn callback_byte_arrays(callbacks: &[CallbackRegistrationView]) -> bool {
        callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| !method.byte_arrays.is_empty())
        })
    }

    fn callback_record_arrays(callbacks: &[CallbackRegistrationView]) -> bool {
        callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| !method.record_arrays.is_empty())
        })
    }

    fn callback_direct_vectors(callbacks: &[CallbackRegistrationView]) -> bool {
        callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| !method.direct_vectors.is_empty())
        })
    }

    fn callback_handles(callbacks: &[CallbackRegistrationView]) -> bool {
        callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| !method.callback_handles.is_empty())
        })
    }

    fn byte_array_returns(
        callbacks: &[CallbackRegistrationView],
        closures: &[ClosureRegistrationView],
    ) -> bool {
        callbacks.iter().any(|callback| {
            callback
                .methods
                .iter()
                .any(|method| method.returns_bytes || method.returns_record)
        }) || closures
            .iter()
            .any(|closure| closure.returns_bytes || closure.returns_record)
    }

    fn record_returns(
        callbacks: &[CallbackRegistrationView],
        closures: &[ClosureRegistrationView],
    ) -> bool {
        callbacks
            .iter()
            .any(|callback| callback.methods.iter().any(|method| method.returns_record))
            || closures.iter().any(|closure| closure.returns_record)
    }
}

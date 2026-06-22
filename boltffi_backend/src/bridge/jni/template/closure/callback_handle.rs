//! Template view for callback-owned closure handles.
//!
//! Callback methods can receive closures from Rust and expose them to the JVM.
//! This module prepares the generated native call and release methods for those
//! closure handles.

use crate::bridge::{
    c::{Expression, Identifier, Literal, Statement, TypeFragment},
    jni::ClosureRegistration,
};

use super::ClosureRegistrationView;

pub struct CallbackClosureHandleView {
    pub ty: Identifier,
    pub new: Identifier,
    pub ref_: Identifier,
    pub release: Identifier,
    pub call_symbol: Identifier,
    pub release_symbol: Identifier,
    pub call_field: Statement,
    pub jni_return_type: TypeFragment,
    pub failure_value: Expression,
    pub closure: ClosureRegistrationView,
}

impl CallbackClosureHandleView {
    pub fn from_registration(registration: &ClosureRegistration) -> Option<Self> {
        registration.callback_handle().map(|handle| Self {
            ty: handle.ty().clone(),
            new: handle.new_function().clone(),
            ref_: handle.ref_function().clone(),
            release: handle.release_function().clone(),
            call_symbol: handle.call_symbol().as_identifier().clone(),
            release_symbol: handle.release_symbol().as_identifier().clone(),
            call_field: handle.call_field().clone(),
            jni_return_type: registration.callback_return_type(),
            failure_value: registration
                .callback_failure_value()
                .unwrap_or_else(|| Expression::literal(Literal::integer_zero())),
            closure: ClosureRegistrationView::from_registration(registration),
        })
    }
}

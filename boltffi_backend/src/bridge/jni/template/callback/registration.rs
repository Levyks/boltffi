//! Template view for callback registrations.
//!
//! Callback registrations provide the generated class name, global method ids,
//! vtable methods, and local protocol symbols needed by the callback templates.

use crate::bridge::{
    c::{Identifier, Literal},
    jni::CallbackRegistration,
};

use super::CallbackMethodView;

pub struct CallbackRegistrationView {
    pub class: Literal,
    pub global_class: Identifier,
    pub free_method: Identifier,
    pub clone_method: Identifier,
    pub load: Identifier,
    pub unload: Identifier,
    pub vtable_type: Identifier,
    pub vtable: Identifier,
    pub register: Identifier,
    pub free: Identifier,
    pub clone: Identifier,
    pub methods: Vec<CallbackMethodView>,
}

impl CallbackRegistrationView {
    pub fn from_registration(registration: &CallbackRegistration) -> Self {
        Self {
            class: Literal::string(&registration.class().as_jni_class_name()),
            global_class: registration.global_class().clone(),
            free_method: registration.free_method().clone(),
            clone_method: registration.clone_method().clone(),
            load: registration.load().clone(),
            unload: registration.unload().clone(),
            vtable_type: registration.vtable_type().clone(),
            vtable: registration.vtable().clone(),
            register: registration.register().clone(),
            free: registration.free().clone(),
            clone: registration.clone_callback().clone(),
            methods: registration
                .methods()
                .iter()
                .map(CallbackMethodView::from_method)
                .collect(),
        }
    }
}

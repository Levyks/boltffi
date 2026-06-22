//! Template view for native method parameters.
//!
//! The native method contract stores typed JNI parameters. This module prepares
//! the small declaration view used by the root native-method template.

use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::NativeParameter,
};

pub struct NativeParameterView {
    pub name: Identifier,
    pub ty: TypeFragment,
}

impl NativeParameterView {
    pub fn from_parameter(parameter: &NativeParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            ty: parameter.ty(),
        }
    }
}

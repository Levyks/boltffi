//! Template view for JNI native method declarations.
//!
//! The contract layer keeps each native parameter tied to its C bridge
//! arguments. The C source declaration only needs the Java-facing name and JNI
//! type. This module projects the contract down to that declaration view.
//!
//! Keeping this tiny view separate matters because method signatures are printed
//! before the body borrows arrays, writes records, checks status, or calls the C
//! bridge. The template receives only the fields that belong in the signature.

use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::NativeParameter,
};

/// One parameter in the generated `Java_*` function signature.
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

//! Template view for closure trampoline C parameters.
//!
//! Closure trampolines are C functions called by Rust. This module prepares the
//! parameter declarations that appear in those generated function signatures.

use crate::bridge::{c::Statement, jni::ClosureCParameter};

pub struct ClosureCParameterView {
    pub declaration: Statement,
}

impl ClosureCParameterView {
    pub fn from_parameter(parameter: ClosureCParameter) -> Self {
        Self {
            declaration: parameter.declaration().clone(),
        }
    }
}

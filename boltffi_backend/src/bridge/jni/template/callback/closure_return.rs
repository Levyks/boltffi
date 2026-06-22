//! Template view for closure returns from callbacks.
//!
//! The callback template needs storage, output pointers, and release expressions
//! when a JVM callback method returns an inline closure. This module prepares
//! that view from the callback contract.

use crate::bridge::{
    c::{Identifier, Statement},
    jni::CallbackClosureReturn,
};

pub struct CallbackClosureReturnView {
    pub output: Identifier,
    pub storage: Identifier,
    pub invoke_field: Statement,
    pub invoke: Identifier,
    pub release: Identifier,
}

impl CallbackClosureReturnView {
    pub fn from_return(returned: &CallbackClosureReturn) -> Self {
        Self {
            output: returned.output().name().clone(),
            storage: returned.storage().clone(),
            invoke_field: returned.invoke_field().clone(),
            invoke: returned.invoke().clone(),
            release: returned.release().clone(),
        }
    }
}

//! Callback handles returned to the JVM.
//!
//! Rust can return callback implementations through the C bridge. This module
//! converts that C handle shape into the JVM token returned by a native method or
//! callback completion.

use boltffi_binding::CallbackId;

use crate::{
    bridge::c::{self, ArgumentList, Expression, Identifier, TypeFragment},
    core::Result,
};

/// JNI callback handle returned as an owned JVM token.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CallbackReturn {
    callback: CallbackId,
}

impl CallbackReturn {
    /// Returns the JNI method return type.
    pub fn jni_type(&self) -> TypeFragment {
        TypeFragment::new("jlong")
    }

    /// Returns the C result type used by the temporary result variable.
    pub fn c_result_type(&self) -> Result<TypeFragment> {
        TypeFragment::anonymous(&c::Type::CallbackHandle(self.callback))
    }

    /// Returns the expression returned from the JNI method.
    pub fn return_expression(&self, value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            Identifier::parse("boltffi_jni_callback_handle_new_owned")?,
            ArgumentList::from_iter([Expression::identifier(Identifier::parse("env")?), value]),
        ))
    }

    /// Creates a callback return from one C callback-handle ABI type.
    pub fn from_c_type(ty: &c::Type) -> Option<Self> {
        match ty {
            c::Type::CallbackHandle(callback) => Some(Self {
                callback: *callback,
            }),
            _ => None,
        }
    }
}

//! Callback vtable slot contract.
//!
//! One Rust callback method becomes one C vtable slot. This module stores the C
//! function signature, cached JVM method id, Java method descriptor, argument
//! conversions, and return handling for that slot.

mod arguments;
mod slot;

use crate::bridge::{
    c::{self, Identifier, TypeFragment},
    jni::{CallbackArgument, CallbackCParameter, CallbackClosureReturn, JvmMethodReturn},
};

/// JNI method dispatch for one callback vtable slot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackMethod {
    function: Identifier,
    method: Identifier,
    method_id: Identifier,
    signature: String,
    returns: JvmMethodReturn,
    c_parameters: Vec<CallbackCParameter>,
    closure_return: Option<CallbackClosureReturn>,
    arguments: Vec<CallbackArgument>,
}

impl CallbackMethod {
    /// Returns the generated C vtable method implementation.
    pub fn function(&self) -> &Identifier {
        &self.function
    }

    /// Returns the JVM static method name.
    pub fn method(&self) -> &Identifier {
        &self.method
    }

    /// Returns the cached JNI method id symbol.
    pub fn method_id(&self) -> &Identifier {
        &self.method_id
    }

    /// Returns the JNI method descriptor.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Returns the C return type for the vtable slot implementation.
    pub fn c_return_type(&self) -> &TypeFragment {
        self.returns.c_type()
    }

    /// Returns whether the slot returns no value.
    pub fn returns_void(&self) -> bool {
        self.returns.is_void()
    }

    /// Returns whether the JVM callback method returns a byte array.
    pub fn returns_byte_array(&self) -> bool {
        self.returns.returns_byte_array()
    }

    /// Returns whether the JVM callback method returns owned encoded bytes.
    pub fn returns_bytes(&self) -> bool {
        self.returns.returns_bytes()
    }

    /// Returns whether the JVM callback method returns a direct record byte array.
    pub fn returns_record(&self) -> bool {
        self.returns.returns_record()
    }

    /// Returns whether the JVM callback method returns a callback handle token.
    pub fn returns_callback_handle(&self) -> bool {
        self.returns.returns_callback_handle()
    }

    /// Returns whether the JVM callback method returns an inline closure handle.
    pub fn returns_closure(&self) -> bool {
        self.returns.returns_closure()
    }

    /// Returns the C callback handle constructor for callback handle returns.
    pub fn callback_handle_constructor(&self) -> Option<&Identifier> {
        self.returns.callback_handle_constructor()
    }

    /// Returns the returned closure out-pointer contract.
    pub fn closure_return(&self) -> Option<&CallbackClosureReturn> {
        self.closure_return.as_ref()
    }

    /// Returns the `CallStatic*Method` suffix for non-void slots.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        self.returns.call_method_suffix()
    }

    /// Returns the C value returned when dispatch fails.
    pub fn failure_value(&self) -> Option<c::Expression> {
        self.returns.failure_value()
    }
}

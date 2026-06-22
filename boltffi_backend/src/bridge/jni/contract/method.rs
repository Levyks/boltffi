//! Native method contracts.
//!
//! Each C bridge function that is callable from the JVM becomes one JNI native
//! method. This contract ties the exported `Java_*` symbol, JNI parameters, JNI
//! return shape, and underlying C bridge function together.

use crate::{
    bridge::{
        c,
        jni::{ClosureRegistration, JniSymbolName, JvmClassPath, NativeParameter, NativeReturn},
    },
    core::Result,
};

/// Native method exported to the JVM by a generated JNI source file.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct NativeMethod {
    c_function: c::Function,
    symbol: JniSymbolName,
    returns: NativeReturn,
    parameters: Vec<NativeParameter>,
}

impl NativeMethod {
    /// Creates a JNI native method from a C function declaration.
    pub fn new(
        class: &JvmClassPath,
        function: &c::Function,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        Ok(Self {
            symbol: JniSymbolName::native_method(class, function.name())?,
            returns: NativeReturn::from_c_type(function.returns())?,
            parameters: NativeParameter::from_c_function(function, callbacks, closures)?,
            c_function: function.clone(),
        })
    }

    /// Returns the C bridge function this method calls.
    pub fn c_function(&self) -> &c::Function {
        &self.c_function
    }

    /// Returns the JNI exported C symbol.
    pub fn symbol(&self) -> &JniSymbolName {
        &self.symbol
    }

    /// Returns the JNI return type.
    pub fn returns(&self) -> &NativeReturn {
        &self.returns
    }

    /// Returns parameters after `JNIEnv*` and `jclass`.
    pub fn parameters(&self) -> &[NativeParameter] {
        &self.parameters
    }

    /// Returns whether this method returns no value.
    pub fn returns_void(&self) -> bool {
        matches!(&self.returns, NativeReturn::Void)
    }

    /// Returns whether this method needs an explicit `jboolean` cast.
    pub fn returns_boolean(&self) -> bool {
        matches!(&self.returns, NativeReturn::Value(scalar) if scalar.jni_type().is_boolean())
    }

    /// Returns whether this method returns an owned byte buffer.
    pub fn returns_bytes(&self) -> bool {
        matches!(&self.returns, NativeReturn::Bytes)
    }

    /// Returns whether this method returns a direct record byte array.
    pub fn returns_record(&self) -> bool {
        matches!(&self.returns, NativeReturn::Record(_))
    }

    /// Returns whether this method returns a callback handle token.
    pub fn returns_callback(&self) -> bool {
        self.returns.is_callback()
    }

    /// Returns whether this method checks a returned `FfiStatus`.
    pub fn checks_status(&self) -> bool {
        matches!(&self.returns, NativeReturn::Status)
    }
}

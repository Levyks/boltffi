//! Return values for JNI native methods.
//!
//! A native method can return void, a scalar, a byte array, a direct record byte
//! array, a callback handle token, or report status only. This module models
//! that JVM-facing return behavior from the C bridge return type.

use crate::{
    bridge::{
        c::{self, Expression, TypeFragment},
        jni::{CallbackReturn, RecordValue, ScalarReturn},
    },
    core::Result,
};

/// JNI return behavior for one native method.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum NativeReturn {
    /// The C function returns `void`.
    Void,
    /// The C function returns a scalar value directly.
    Value(ScalarReturn),
    /// The C function returns an owned BoltFFI byte buffer.
    Bytes,
    /// The C function returns a direct record by value.
    Record(RecordValue),
    /// The C function returns a callback handle by value.
    Callback(CallbackReturn),
    /// The C function returns `FfiStatus` and the JNI method returns `void`.
    Status,
}

impl NativeReturn {
    /// Returns the JNI method return type as C syntax.
    pub fn jni_type(&self) -> TypeFragment {
        match self {
            Self::Void | Self::Status => TypeFragment::new("void"),
            Self::Value(scalar) => scalar.jni_type().as_type_fragment(),
            Self::Callback(callback) => callback.jni_type(),
            Self::Bytes | Self::Record(_) => TypeFragment::new("jbyteArray"),
        }
    }

    /// Returns the temporary C result type used inside the JNI body.
    pub fn c_result_type(&self) -> Result<TypeFragment> {
        match self {
            Self::Void => Ok(TypeFragment::new("void")),
            Self::Status => TypeFragment::anonymous(&c::Type::Status),
            Self::Value(scalar) => scalar.c_result_type(),
            Self::Bytes => TypeFragment::anonymous(&c::Type::Buffer),
            Self::Record(record) => Ok(record.c_type_fragment()),
            Self::Callback(callback) => callback.c_result_type(),
        }
    }

    /// Returns the expression returned from the JNI method for scalar values.
    pub fn return_expression(&self, value: Expression) -> Result<Expression> {
        match self {
            Self::Value(scalar) => Ok(scalar.return_expression(value)),
            Self::Callback(callback) => callback.return_expression(value),
            Self::Void | Self::Bytes | Self::Record(_) | Self::Status => Ok(value),
        }
    }

    /// Returns direct-record return details when this return carries a record.
    pub fn record(&self) -> Option<&RecordValue> {
        match self {
            Self::Void | Self::Value(_) | Self::Bytes | Self::Callback(_) | Self::Status => None,
            Self::Record(record) => Some(record),
        }
    }

    /// Returns whether this return carries a callback handle.
    pub fn is_callback(&self) -> bool {
        matches!(self, Self::Callback(_))
    }

    /// Creates the JNI return behavior for a C ABI return type.
    pub fn from_c_type(ty: &c::Type) -> Result<Self> {
        if let Some(record) = RecordValue::from_c_type(ty) {
            return Ok(Self::Record(record));
        }
        if let Some(callback) = CallbackReturn::from_c_type(ty) {
            return Ok(Self::Callback(callback));
        }
        match ty {
            c::Type::Void => Ok(Self::Void),
            c::Type::Status => Ok(Self::Status),
            c::Type::Buffer => Ok(Self::Bytes),
            ty => ScalarReturn::from_c_type(ty).map(Self::Value),
        }
    }
}

use crate::bridge::{
    c::{Expression, Literal},
    jni::JvmMethodReturn,
};

impl JvmMethodReturn {
    /// Returns the C expression used when JVM dispatch fails.
    pub fn failure_value(&self) -> Option<Expression> {
        match self {
            Self::Void { .. } => None,
            Self::Value { jni_type, .. } => Some(Expression::literal(jni_type.failure_value())),
            Self::Bytes { c_type }
            | Self::Record { c_type }
            | Self::CallbackHandle { c_type, .. } => Some(Expression::cast(
                c_type.clone(),
                Expression::literal(Literal::compound_zero()),
            )),
            Self::Closure { c_type } => Some(Expression::cast(
                c_type.clone(),
                Expression::literal(Literal::status_failure()),
            )),
        }
    }

    /// Returns the JNI expression used when a Rust-owned closure call fails.
    pub fn jni_failure_value(&self) -> Option<Expression> {
        match self {
            Self::Void { .. } => None,
            Self::Value { jni_type, .. } => Some(Expression::literal(jni_type.failure_value())),
            Self::Bytes { .. } | Self::Record { .. } => {
                Some(Expression::literal(Literal::null_pointer()))
            }
            Self::CallbackHandle { .. } | Self::Closure { .. } => {
                Some(Expression::literal(Literal::integer_zero()))
            }
        }
    }
}

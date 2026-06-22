//! Scalar arguments passed into JVM-owned closures.
//!
//! Scalar closure arguments keep the C parameter and JNI primitive type together
//! so the trampoline can cast before calling the JVM method.

use crate::{
    bridge::{
        c::{self, Expression},
        jni::{ClosureCParameter, JniType},
    },
    core::Result,
};

/// Scalar inline-closure argument crossing the JNI bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureScalarArgument {
    parameter: ClosureCParameter,
    jni_type: JniType,
}

impl ClosureScalarArgument {
    pub(in crate::bridge::jni::contract::closure) fn from_parameter(
        parameter: &c::Parameter,
    ) -> Result<Self> {
        Ok(Self {
            parameter: ClosureCParameter::from_parameter(parameter)?,
            jni_type: JniType::from_c_type(parameter.ty())?,
        })
    }

    /// Returns the C parameters accepted by the closure call trampoline.
    pub fn c_parameters(&self) -> Vec<ClosureCParameter> {
        vec![self.parameter.clone()]
    }

    /// Returns the C parameters accepted by the Rust-owned closure handle entrypoint.
    pub fn handle_parameters(&self) -> Vec<ClosureCParameter> {
        vec![ClosureCParameter::new(
            self.parameter.name().clone(),
            self.jni_type.as_type_fragment(),
        )]
    }

    /// Returns the expressions passed to the static JVM closure method.
    pub fn jvm_arguments(&self) -> Vec<Expression> {
        vec![Expression::cast(
            self.jni_type.as_type_fragment(),
            Expression::identifier(self.parameter.name().clone()),
        )]
    }

    /// Returns the expressions passed into the Rust closure call function.
    pub fn rust_arguments(&self) -> Vec<Expression> {
        vec![Expression::cast(
            self.parameter.ty().clone(),
            Expression::identifier(self.parameter.name().clone()),
        )]
    }

    /// Returns the JNI method descriptor segment for this argument.
    pub fn jni_signature(&self) -> &'static str {
        self.jni_type.signature()
    }
}

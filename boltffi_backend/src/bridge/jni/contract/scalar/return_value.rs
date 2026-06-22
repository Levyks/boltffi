use crate::{
    bridge::{
        c::{self, Expression, TypeFragment},
        jni::JniType,
    },
    core::Result,
};

/// Scalar JNI return value mapped from one scalar C bridge return.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct ScalarReturn {
    c_type: c::Type,
    jni_type: JniType,
}

impl ScalarReturn {
    /// Returns the scalar JNI return type.
    pub fn jni_type(&self) -> JniType {
        self.jni_type
    }

    /// Returns the C bridge result type used by the temporary result variable.
    pub fn c_result_type(&self) -> Result<TypeFragment> {
        TypeFragment::anonymous(&self.c_type)
    }

    /// Returns the expression returned from the JNI method.
    pub fn return_expression(&self, value: Expression) -> Expression {
        Expression::cast(self.jni_type.as_type_fragment(), value)
    }

    /// Creates a scalar JNI return from one scalar C ABI return type.
    pub fn from_c_type(ty: &c::Type) -> Result<Self> {
        Ok(Self {
            c_type: ty.clone(),
            jni_type: JniType::from_c_type(ty)?,
        })
    }
}

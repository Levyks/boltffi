use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::ScalarParameter,
    },
    core::Result,
};

/// JNI parameter that supplies callback data for a C poll continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ContinuationParameter {
    data: ScalarParameter,
}

impl ContinuationParameter {
    /// Returns the generated JNI parameter name.
    pub fn name(&self) -> &Identifier {
        self.data.name()
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        self.data.ty().as_type_fragment()
    }

    /// Returns C bridge call arguments produced from this JNI parameter.
    pub fn c_arguments(&self) -> Result<Vec<Expression>> {
        Ok(vec![
            self.data.c_argument()?,
            Expression::identifier(Identifier::parse("boltffi_jni_continuation_callback")?),
        ])
    }

    /// Creates a JNI continuation parameter from a C continuation parameter group.
    pub fn from_c_group(group: &c::ContinuationParameter, function: &c::Function) -> Result<Self> {
        ScalarParameter::from_c_parameter(function.parameter(group.data()))
            .map(|data| Self { data })
    }
}

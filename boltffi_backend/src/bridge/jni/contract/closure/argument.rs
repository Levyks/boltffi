use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::JniType,
    },
    core::Result,
};

/// One C closure argument forwarded to a JVM closure bridge method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureArgument {
    name: Identifier,
    c_type: TypeFragment,
    jni_type: JniType,
}

impl ClosureArgument {
    /// Returns the generated C argument name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C argument type.
    pub fn c_type(&self) -> &TypeFragment {
        &self.c_type
    }

    /// Returns the JNI type used when calling Java.
    pub fn jni_type(&self) -> TypeFragment {
        self.jni_type.as_type_fragment()
    }

    /// Returns the JNI method descriptor segment for this argument.
    pub fn jni_signature(&self) -> &'static str {
        self.jni_type.signature()
    }

    /// Creates a closure argument from one C function-pointer parameter.
    pub fn from_c_type((index, ty): (usize, &c::Type)) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(format!("arg{index}"))?,
            c_type: TypeFragment::anonymous(ty)?,
            jni_type: JniType::from_c_type(ty)?,
        })
    }
}

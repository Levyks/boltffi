use crate::{
    bridge::{
        c::{self, TypeFragment},
        jni::JniType,
    },
    core::Result,
};

/// JNI return behavior for one native method.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum NativeReturn {
    /// The C function returns `void`.
    Void,
    /// The C function returns a scalar value directly.
    Value(JniType),
    /// The C function returns an owned BoltFFI byte buffer.
    Bytes,
    /// The C function returns `FfiStatus` and the JNI method returns `void`.
    Status,
}

impl NativeReturn {
    /// Returns the JNI method return type as C syntax.
    pub fn jni_type(self) -> TypeFragment {
        match self {
            Self::Void | Self::Status => TypeFragment::new("void"),
            Self::Value(ty) => ty.as_type_fragment(),
            Self::Bytes => TypeFragment::new("jbyteArray"),
        }
    }

    /// Returns the temporary C result type used inside the JNI body.
    pub fn c_result_type(self) -> Result<TypeFragment> {
        match self {
            Self::Void => Ok(TypeFragment::new("void")),
            Self::Status => TypeFragment::anonymous(&c::Type::Status),
            Self::Value(ty) => Ok(ty.as_type_fragment()),
            Self::Bytes => TypeFragment::anonymous(&c::Type::Buffer),
        }
    }

    /// Creates the JNI return behavior for a C ABI return type.
    pub fn from_c_type(ty: &c::Type) -> Result<Self> {
        match ty {
            c::Type::Void => Ok(Self::Void),
            c::Type::Status => Ok(Self::Status),
            c::Type::Buffer => Ok(Self::Bytes),
            ty => JniType::from_c_type(ty).map(Self::Value),
        }
    }
}

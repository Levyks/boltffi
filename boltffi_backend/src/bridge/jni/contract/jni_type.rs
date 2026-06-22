use crate::{
    bridge::c::{self, Literal, TypeFragment},
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI scalar type used in a native method signature.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum JniType {
    /// `jboolean`.
    Boolean,
    /// `jbyte`.
    Byte,
    /// `jshort`.
    Short,
    /// `jint`.
    Int,
    /// `jlong`.
    Long,
    /// `jfloat`.
    Float,
    /// `jdouble`.
    Double,
}

impl JniType {
    /// Returns the JNI type as C syntax.
    pub fn as_type_fragment(self) -> TypeFragment {
        TypeFragment::new(match self {
            Self::Boolean => "jboolean",
            Self::Byte => "jbyte",
            Self::Short => "jshort",
            Self::Int => "jint",
            Self::Long => "jlong",
            Self::Float => "jfloat",
            Self::Double => "jdouble",
        })
    }

    /// Returns the JNI array type as C syntax.
    pub fn as_array_type_fragment(self) -> TypeFragment {
        TypeFragment::new(match self {
            Self::Boolean => "jbooleanArray",
            Self::Byte => "jbyteArray",
            Self::Short => "jshortArray",
            Self::Int => "jintArray",
            Self::Long => "jlongArray",
            Self::Float => "jfloatArray",
            Self::Double => "jdoubleArray",
        })
    }

    /// Returns the `Get*ArrayElements` JNI function table member.
    pub fn array_elements_getter(self) -> &'static str {
        match self {
            Self::Boolean => "GetBooleanArrayElements",
            Self::Byte => "GetByteArrayElements",
            Self::Short => "GetShortArrayElements",
            Self::Int => "GetIntArrayElements",
            Self::Long => "GetLongArrayElements",
            Self::Float => "GetFloatArrayElements",
            Self::Double => "GetDoubleArrayElements",
        }
    }

    /// Returns the `Release*ArrayElements` JNI function table member.
    pub fn array_elements_releaser(self) -> &'static str {
        match self {
            Self::Boolean => "ReleaseBooleanArrayElements",
            Self::Byte => "ReleaseByteArrayElements",
            Self::Short => "ReleaseShortArrayElements",
            Self::Int => "ReleaseIntArrayElements",
            Self::Long => "ReleaseLongArrayElements",
            Self::Float => "ReleaseFloatArrayElements",
            Self::Double => "ReleaseDoubleArrayElements",
        }
    }

    /// Returns whether this type is `jboolean`.
    pub fn is_boolean(self) -> bool {
        matches!(self, Self::Boolean)
    }

    /// Returns the JNI type descriptor used in method signatures.
    pub fn signature(self) -> &'static str {
        match self {
            Self::Boolean => "Z",
            Self::Byte => "B",
            Self::Short => "S",
            Self::Int => "I",
            Self::Long => "J",
            Self::Float => "F",
            Self::Double => "D",
        }
    }

    /// Returns the JNI array descriptor for this scalar type.
    pub fn array_signature(self) -> &'static str {
        match self {
            Self::Boolean => "[Z",
            Self::Byte => "[B",
            Self::Short => "[S",
            Self::Int => "[I",
            Self::Long => "[J",
            Self::Float => "[F",
            Self::Double => "[D",
        }
    }

    /// Returns the `New*Array` JNI function table member.
    pub fn new_array(self) -> &'static str {
        match self {
            Self::Boolean => "NewBooleanArray",
            Self::Byte => "NewByteArray",
            Self::Short => "NewShortArray",
            Self::Int => "NewIntArray",
            Self::Long => "NewLongArray",
            Self::Float => "NewFloatArray",
            Self::Double => "NewDoubleArray",
        }
    }

    /// Returns the `Set*ArrayRegion` JNI function table member.
    pub fn set_array_region(self) -> &'static str {
        match self {
            Self::Boolean => "SetBooleanArrayRegion",
            Self::Byte => "SetByteArrayRegion",
            Self::Short => "SetShortArrayRegion",
            Self::Int => "SetIntArrayRegion",
            Self::Long => "SetLongArrayRegion",
            Self::Float => "SetFloatArrayRegion",
            Self::Double => "SetDoubleArrayRegion",
        }
    }

    /// Returns the `CallStatic*Method` suffix for this JNI scalar type.
    pub fn call_method_suffix(self) -> &'static str {
        match self {
            Self::Boolean => "Boolean",
            Self::Byte => "Byte",
            Self::Short => "Short",
            Self::Int => "Int",
            Self::Long => "Long",
            Self::Float => "Float",
            Self::Double => "Double",
        }
    }

    /// Returns the C expression used when callback dispatch fails.
    pub fn failure_value(self) -> Literal {
        match self {
            Self::Boolean => Literal::bool_false(),
            Self::Byte | Self::Short | Self::Int | Self::Long => Literal::integer_zero(),
            Self::Float => Literal::f32_zero(),
            Self::Double => Literal::f64_zero(),
        }
    }

    /// Creates the JNI scalar type for a scalar C ABI type.
    pub fn from_c_type(ty: &c::Type) -> Result<Self> {
        match ty {
            c::Type::Bool => Ok(Self::Boolean),
            c::Type::CStyleEnum { repr, .. } => Self::from_c_type(repr),
            c::Type::Int8 | c::Type::Uint8 | c::Type::StreamPollResult => Ok(Self::Byte),
            c::Type::Int16 | c::Type::Uint16 => Ok(Self::Short),
            c::Type::Int32 | c::Type::Uint32 | c::Type::WaitResult => Ok(Self::Int),
            c::Type::Int64
            | c::Type::Uint64
            | c::Type::SignedPointerWidth
            | c::Type::PointerWidth
            | c::Type::FutureHandle
            | c::Type::ConstPointer(_)
            | c::Type::MutPointer(_)
            | c::Type::FunctionPointer { .. } => Ok(Self::Long),
            c::Type::Float32 => Ok(Self::Float),
            c::Type::Float64 => Ok(Self::Double),
            c::Type::CallbackHandle(_) => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "callback handle C ABI",
            }),
            c::Type::Void
            | c::Type::Status
            | c::Type::Buffer
            | c::Type::String
            | c::Type::Span
            | c::Type::Named(_)
            | c::Type::DirectRecord(_) => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "non-scalar C ABI function",
            }),
        }
    }
}

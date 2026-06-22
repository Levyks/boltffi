use crate::{
    bridge::{
        c::{self, Expression, Identifier, Literal, TypeFragment},
        jni::JniType,
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// Return contract for a static JVM method called from generated C.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum JvmMethodReturn {
    /// The JVM method returns `void`.
    Void {
        /// C return type of the generated trampoline.
        c_type: TypeFragment,
    },
    /// The JVM method returns one JNI scalar value.
    Value {
        /// C return type of the generated trampoline.
        c_type: TypeFragment,
        /// JNI scalar type returned by `CallStatic*Method`.
        jni_type: JniType,
    },
    /// The JVM method returns a Java byte array copied into `FfiBuf_u8`.
    Bytes {
        /// C return type of the generated trampoline.
        c_type: TypeFragment,
    },
    /// The JVM method returns a Java byte array copied into one C record.
    Record {
        /// C return type of the generated trampoline.
        c_type: TypeFragment,
    },
    /// The JVM method returns a callback object handle.
    CallbackHandle {
        /// C return type of the generated trampoline.
        c_type: TypeFragment,
        /// C callback handle constructor.
        create_handle: Identifier,
    },
    /// The JVM method returns a closure handle and the C trampoline returns `FfiStatus`.
    Closure {
        /// C return type of the generated trampoline.
        c_type: TypeFragment,
    },
}

impl JvmMethodReturn {
    /// Creates a JVM method return contract from one C ABI return type.
    pub fn from_c_type(ty: &c::Type, callbacks: &[c::Callback]) -> Result<Self> {
        match ty {
            c::Type::Void => Ok(Self::Void {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            c::Type::Buffer => Ok(Self::Bytes {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            c::Type::DirectRecord(_) => Ok(Self::Record {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            c::Type::CallbackHandle(callback) => {
                let declaration = callbacks
                    .iter()
                    .find(|declaration| declaration.id() == *callback)
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant: "JVM callback handle return has no C callback declaration",
                    })?;
                Ok(Self::CallbackHandle {
                    c_type: TypeFragment::anonymous(ty)?,
                    create_handle: Identifier::parse(declaration.create_handle().name())?,
                })
            }
            ty => Ok(Self::Value {
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: JniType::from_c_type(ty)?,
            }),
        }
    }

    /// Creates the return contract for closure-return callback methods.
    pub fn closure_status() -> Result<Self> {
        Ok(Self::Closure {
            c_type: TypeFragment::anonymous(&c::Type::Status)?,
        })
    }

    /// Returns the generated C return type.
    pub fn c_type(&self) -> &TypeFragment {
        match self {
            Self::Void { c_type }
            | Self::Value { c_type, .. }
            | Self::Bytes { c_type }
            | Self::Record { c_type }
            | Self::CallbackHandle { c_type, .. }
            | Self::Closure { c_type } => c_type,
        }
    }

    /// Returns the JNI C return type.
    pub fn jni_type(&self) -> TypeFragment {
        match self {
            Self::Void { c_type } => c_type.clone(),
            Self::Value { jni_type, .. } => jni_type.as_type_fragment(),
            Self::Bytes { .. } | Self::Record { .. } => TypeFragment::new("jbyteArray"),
            Self::CallbackHandle { .. } => TypeFragment::new("jlong"),
            Self::Closure { .. } => TypeFragment::new("jlong"),
        }
    }

    /// Returns the JNI method descriptor return segment.
    pub fn signature(&self) -> &'static str {
        match self {
            Self::Void { .. } => "V",
            Self::Value { jni_type, .. } => jni_type.signature(),
            Self::Bytes { .. } | Self::Record { .. } => "[B",
            Self::CallbackHandle { .. } => "J",
            Self::Closure { .. } => "J",
        }
    }

    /// Returns whether the static JVM method returns no value.
    pub fn is_void(&self) -> bool {
        matches!(self, Self::Void { .. })
    }

    /// Returns whether the Java return value is a byte array.
    pub fn returns_byte_array(&self) -> bool {
        matches!(self, Self::Bytes { .. } | Self::Record { .. })
    }

    /// Returns whether the byte-array return value is copied into `FfiBuf_u8`.
    pub fn returns_bytes(&self) -> bool {
        matches!(self, Self::Bytes { .. })
    }

    /// Returns whether the byte-array return value is copied into a direct record.
    pub fn returns_record(&self) -> bool {
        matches!(self, Self::Record { .. })
    }

    /// Returns whether the JVM method returns a callback handle token.
    pub fn returns_callback_handle(&self) -> bool {
        matches!(self, Self::CallbackHandle { .. })
    }

    /// Returns whether the JVM method returns an inline closure handle.
    pub fn returns_closure(&self) -> bool {
        matches!(self, Self::Closure { .. })
    }

    /// Returns the C callback handle constructor for callback handle returns.
    pub fn callback_handle_constructor(&self) -> Option<&Identifier> {
        match self {
            Self::CallbackHandle { create_handle, .. } => Some(create_handle),
            Self::Void { .. }
            | Self::Value { .. }
            | Self::Bytes { .. }
            | Self::Record { .. }
            | Self::Closure { .. } => None,
        }
    }

    /// Returns the `CallStatic*Method` suffix for non-void returns.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        match self {
            Self::Void { .. } => None,
            Self::Value { jni_type, .. } => Some(jni_type.call_method_suffix()),
            Self::Bytes { .. } | Self::Record { .. } => Some("Object"),
            Self::CallbackHandle { .. } => Some("Long"),
            Self::Closure { .. } => Some("Long"),
        }
    }

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

    /// Returns the JNI expression used when dispatch fails.
    pub fn jni_failure_value(&self) -> Option<Expression> {
        match self {
            Self::Void { .. } => None,
            Self::Value { jni_type, .. } => Some(Expression::literal(jni_type.failure_value())),
            Self::Bytes { .. } | Self::Record { .. } => {
                Some(Expression::literal(Literal::null_pointer()))
            }
            Self::CallbackHandle { .. } => Some(Expression::literal(Literal::integer_zero())),
            Self::Closure { .. } => Some(Expression::literal(Literal::integer_zero())),
        }
    }
}

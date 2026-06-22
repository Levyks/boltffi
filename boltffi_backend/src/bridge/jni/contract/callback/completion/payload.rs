//! Success payloads for async callback completion.
//!
//! A completed async callback can return void, a scalar, owned encoded bytes, a
//! direct record, or a callback handle. This module models that payload before
//! the generated completion method forwards it to Rust.

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::JniType,
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// Successful async callback completion payload carried from the JVM to Rust.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionPayload {
    suffix: String,
    c_type: TypeFragment,
    jni_type: TypeFragment,
    kind: CallbackCompletionPayloadKind,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum CallbackCompletionPayloadKind {
    Scalar,
    Bytes,
    Record,
    CallbackHandle { create_handle: Identifier },
}

impl CallbackCompletionPayload {
    /// Creates a completion payload from the C completion function pointer payload type.
    pub fn from_c_type(ty: &c::Type, callbacks: &[c::Callback]) -> Result<Self> {
        match ty {
            c::Type::Buffer => Ok(Self {
                suffix: "Bytes".to_owned(),
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: TypeFragment::new("jbyteArray"),
                kind: CallbackCompletionPayloadKind::Bytes,
            }),
            c::Type::DirectRecord(name) => Ok(Self {
                suffix: format!("Record_{}", name.as_str()),
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: TypeFragment::new("jbyteArray"),
                kind: CallbackCompletionPayloadKind::Record,
            }),
            c::Type::CallbackHandle(callback) => {
                let declaration = callbacks
                    .iter()
                    .find(|declaration| declaration.id() == *callback)
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant: "async callback completion payload has no C callback declaration",
                    })?;
                let c_type = TypeFragment::anonymous(ty)?;
                Ok(Self {
                    suffix: format!("Callback_{c_type}"),
                    c_type,
                    jni_type: TypeFragment::new("jlong"),
                    kind: CallbackCompletionPayloadKind::CallbackHandle {
                        create_handle: Identifier::parse(declaration.create_handle().name())?,
                    },
                })
            }
            c::Type::Void
            | c::Type::Status
            | c::Type::String
            | c::Type::Span
            | c::Type::FutureHandle
            | c::Type::Named(_)
            | c::Type::ConstPointer(_)
            | c::Type::MutPointer(_)
            | c::Type::FunctionPointer { .. } => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "async callback completion payload",
            }),
            ty => Ok(Self {
                suffix: Self::scalar_suffix(ty)?,
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: JniType::from_c_type(ty)?.as_type_fragment(),
                kind: CallbackCompletionPayloadKind::Scalar,
            }),
        }
    }

    /// Returns the suffix used to deduplicate generated completion invokers.
    pub fn suffix(&self) -> &str {
        &self.suffix
    }

    /// Returns the C payload type passed to the completion function pointer.
    pub fn c_type(&self) -> &TypeFragment {
        &self.c_type
    }

    /// Returns the JNI parameter type accepted by the success invoker.
    pub fn jni_type(&self) -> &TypeFragment {
        &self.jni_type
    }

    /// Returns whether the payload is an owned byte buffer.
    pub fn is_bytes(&self) -> bool {
        matches!(self.kind, CallbackCompletionPayloadKind::Bytes)
    }

    /// Returns whether the payload is a direct record value.
    pub fn is_record(&self) -> bool {
        matches!(self.kind, CallbackCompletionPayloadKind::Record)
    }

    /// Returns the C callback constructor needed for callback-handle payloads.
    pub fn callback_handle_constructor(&self) -> Option<&Identifier> {
        match &self.kind {
            CallbackCompletionPayloadKind::CallbackHandle { create_handle } => Some(create_handle),
            _ => None,
        }
    }

    fn scalar_suffix(ty: &c::Type) -> Result<String> {
        Ok(match ty {
            c::Type::Bool => "Bool".to_owned(),
            c::Type::Int8 => "I8".to_owned(),
            c::Type::Uint8 | c::Type::StreamPollResult => "U8".to_owned(),
            c::Type::Int16 => "I16".to_owned(),
            c::Type::Uint16 => "U16".to_owned(),
            c::Type::Int32 => "I32".to_owned(),
            c::Type::Uint32 | c::Type::WaitResult => "U32".to_owned(),
            c::Type::Int64 => "I64".to_owned(),
            c::Type::Uint64 => "U64".to_owned(),
            c::Type::SignedPointerWidth => "ISize".to_owned(),
            c::Type::PointerWidth => "USize".to_owned(),
            c::Type::Float32 => "F32".to_owned(),
            c::Type::Float64 => "F64".to_owned(),
            c::Type::CStyleEnum { name, .. } => format!("Enum_{}", name.as_str()),
            _ => {
                return Err(Error::UnsupportedBridge {
                    bridge: JNI_BRIDGE,
                    shape: "scalar async callback completion payload",
                });
            }
        })
    }
}

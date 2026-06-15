use boltffi_binding::{CodecNode, Primitive};

use crate::experimental::error::Error;

pub mod incoming;
pub mod outgoing;

mod custom;

pub use self::custom::{BorrowedOutgoing, Incoming};

pub fn require_runtime_wire(codec: &CodecNode) -> Result<(), Error> {
    RuntimeWireCodec::new(codec).require_supported()
}

struct RuntimeWireCodec<'codec> {
    codec: &'codec CodecNode,
}

impl<'codec> RuntimeWireCodec<'codec> {
    const MAX_TUPLE_ARITY: usize = 12;

    const fn new(codec: &'codec CodecNode) -> Self {
        Self { codec }
    }

    fn require_supported(self) -> Result<(), Error> {
        self.require_supported_codec(self.codec)
    }

    fn require_supported_codec(&self, codec: &CodecNode) -> Result<(), Error> {
        match codec {
            CodecNode::Primitive(_)
            | CodecNode::String
            | CodecNode::Bytes
            | CodecNode::DirectRecord(_)
            | CodecNode::EncodedRecord(_)
            | CodecNode::CStyleEnum(_)
            | CodecNode::DataEnum(_)
            | CodecNode::Custom(_) => Ok(()),
            CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
                self.require_supported_codec(inner)
            }
            CodecNode::Result { ok, err } => {
                self.require_supported_codec(ok)?;
                self.require_supported_codec(err)
            }
            CodecNode::Tuple(elements) if elements.len() <= Self::MAX_TUPLE_ARITY => elements
                .iter()
                .try_for_each(|element| self.require_supported_codec(element)),
            CodecNode::Tuple(_) => Err(Error::UnsupportedExpansion("encoded tuple arity")),
            CodecNode::Map { key, value } => {
                self.require_supported_map_key(key)?;
                self.require_supported_codec(value)
            }
            CodecNode::ClassHandle(_) | CodecNode::CallbackHandle(_) | _ => {
                Err(Error::UnsupportedExpansion("codec node"))
            }
        }
    }

    fn require_supported_map_key(&self, codec: &CodecNode) -> Result<(), Error> {
        match codec {
            CodecNode::Primitive(Primitive::F32 | Primitive::F64) => Err(
                Error::UnsupportedExpansion("floating-point encoded map key"),
            ),
            CodecNode::Custom(_) => Err(Error::UnsupportedExpansion("custom encoded map key")),
            CodecNode::Map { .. } => Err(Error::UnsupportedExpansion("nested encoded map key")),
            CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
                self.require_supported_map_key(inner)
            }
            CodecNode::Result { ok, err } => {
                self.require_supported_map_key(ok)?;
                self.require_supported_map_key(err)
            }
            CodecNode::Tuple(elements) if elements.len() <= Self::MAX_TUPLE_ARITY => elements
                .iter()
                .try_for_each(|element| self.require_supported_map_key(element)),
            CodecNode::Tuple(_) => Err(Error::UnsupportedExpansion("encoded tuple arity")),
            _ => self.require_supported_codec(codec),
        }
    }
}

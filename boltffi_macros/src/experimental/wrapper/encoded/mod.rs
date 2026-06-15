use boltffi_binding::CodecNode;

use crate::experimental::error::Error;

pub mod incoming;
pub mod outgoing;

mod custom;

pub use self::custom::{BorrowedOutgoing, Incoming};

pub fn require_runtime_wire(codec: &CodecNode) -> Result<(), Error> {
    if uses_runtime_wire(codec) {
        Ok(())
    } else {
        Err(Error::UnsupportedExpansion("codec node"))
    }
}

fn uses_runtime_wire(codec: &CodecNode) -> bool {
    match codec {
        CodecNode::Primitive(_)
        | CodecNode::String
        | CodecNode::Bytes
        | CodecNode::DirectRecord(_)
        | CodecNode::EncodedRecord(_)
        | CodecNode::CStyleEnum(_)
        | CodecNode::DataEnum(_)
        | CodecNode::Custom(_) => true,
        CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
            uses_runtime_wire(inner)
        }
        CodecNode::Result { ok, err } => uses_runtime_wire(ok) && uses_runtime_wire(err),
        CodecNode::Tuple(elements) => elements.iter().all(uses_runtime_wire),
        CodecNode::Map { key, value } => uses_runtime_wire(key) && uses_runtime_wire(value),
        CodecNode::ClassHandle(_) | CodecNode::CallbackHandle(_) | _ => false,
    }
}

use boltffi_binding::CodecNode;

use crate::experimental::error::Error;

pub mod incoming;
pub mod outgoing;

fn require_runtime_wire(root: &CodecNode) -> Result<(), Error> {
    if uses_runtime_wire(root) {
        Ok(())
    } else {
        Err(Error::UnsupportedExpansion("codec node"))
    }
}

fn uses_runtime_wire(root: &CodecNode) -> bool {
    match root {
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
        CodecNode::Tuple(_)
        | CodecNode::Map { .. }
        | CodecNode::ClassHandle(_)
        | CodecNode::CallbackHandle(_)
        | _ => false,
    }
}

use boltffi_binding::{Native, OutOfRust, ReturnPlan, TypeRef, native};

use crate::{
    core::{Error, Result},
    target::python::cpython::render::primitive,
};

pub struct Conversion {
    pub void: bool,
    pub converter: &'static str,
    primitive: Option<primitive::Runtime>,
    owned_buffer: Option<OwnedBuffer>,
}

impl Conversion {
    pub fn from_plan(plan: &ReturnPlan<Native, OutOfRust>) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self {
                void: true,
                converter: "",
                primitive: None,
                owned_buffer: None,
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let primitive = primitive::Runtime::new(*primitive);
                Ok(Self {
                    void: false,
                    converter: primitive.boxer()?,
                    primitive: Some(primitive),
                    owned_buffer: None,
                })
            }
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::from_owned_buffer(OwnedBuffer::String)),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::from_owned_buffer(OwnedBuffer::Bytes)),
            ReturnPlan::DirectViaReturnSlot { .. }
            | ReturnPlan::EncodedViaReturnSlot { .. }
            | ReturnPlan::HandleViaReturnSlot { .. }
            | ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. }
            | ReturnPlan::DirectViaOutPointer { .. }
            | ReturnPlan::EncodedViaOutPointer { .. }
            | ReturnPlan::HandleViaOutPointer { .. }
            | ReturnPlan::ClosureViaOutPointer(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "non-primitive return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown return plan",
            }),
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    pub fn owned_buffer(&self) -> Option<OwnedBuffer> {
        self.owned_buffer
    }

    pub fn is_void(&self) -> bool {
        self.void
    }

    fn from_owned_buffer(buffer: OwnedBuffer) -> Self {
        Self {
            void: false,
            converter: buffer.converter(),
            primitive: None,
            owned_buffer: Some(buffer),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OwnedBuffer {
    String,
    Bytes,
}

impl OwnedBuffer {
    pub fn converter(self) -> &'static str {
        match self {
            Self::String => "boltffi_python_decode_owned_utf8",
            Self::Bytes => "boltffi_python_decode_owned_bytes",
        }
    }
}

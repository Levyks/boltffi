use boltffi_binding::{
    HandlePresence, HandleTarget, Native, OutOfRust, ReturnPlan, TypeRef, native,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, RenderContext, Result},
    target::python::cpython::render::{enumeration, primitive, record, scalar_handle},
};

pub struct Conversion {
    pub void: bool,
    pub converter: String,
    primitive: Option<primitive::Runtime>,
    owned_buffer: Option<OwnedBuffer>,
}

impl Conversion {
    pub fn from_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self {
                void: true,
                converter: String::new(),
                primitive: None,
                owned_buffer: None,
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let primitive = primitive::Runtime::new(*primitive);
                Ok(Self {
                    void: false,
                    converter: primitive.boxer()?.to_owned(),
                    primitive: Some(primitive),
                    owned_buffer: None,
                })
            }
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Record(record),
            } => Ok(Self {
                void: false,
                converter: record::Symbols::from_record_id(*record, bridge, context)?
                    .boxer()
                    .to_owned(),
                primitive: None,
                owned_buffer: None,
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Enum(enumeration),
            } => Ok(Self {
                void: false,
                converter: enumeration::Symbols::from_enum_id(*enumeration, bridge, context)?
                    .boxer()
                    .to_owned(),
                primitive: None,
                owned_buffer: None,
            }),
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
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(_),
                carrier,
                presence: HandlePresence::Required,
            } => {
                let carrier = scalar_handle::Carrier::new(*carrier)?;
                Ok(Self {
                    void: false,
                    converter: carrier.boxer()?.to_owned(),
                    primitive: Some(carrier.primitive()),
                    owned_buffer: None,
                })
            }
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
            converter: buffer.converter().to_owned(),
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

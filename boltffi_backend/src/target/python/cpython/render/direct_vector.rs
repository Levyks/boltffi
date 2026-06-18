use boltffi_binding::{Native, TypeRef};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{RenderContext, Result},
    target::python::cpython::render::{direct, primitive},
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Element {
    slot: direct::NativeSlot,
    vector_boxer: String,
    vector_encoder: String,
    vector_parser: String,
    vector_decoder: String,
}

impl Element {
    pub fn from_type_ref(
        ty: &TypeRef,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let slot = direct::NativeSlot::from_type_ref(
            ty,
            bridge,
            context,
            "unsupported direct vector element",
        )?;
        Self::from_native_slot(slot)
    }

    pub fn c_type(&self) -> &str {
        self.slot.c_type()
    }

    pub fn parser(&self) -> &str {
        self.slot.parser()
    }

    pub fn boxer(&self) -> &str {
        self.slot.boxer()
    }

    pub fn vector_boxer(&self) -> &str {
        &self.vector_boxer
    }

    pub fn vector_encoder(&self) -> &str {
        &self.vector_encoder
    }

    pub fn vector_parser(&self) -> &str {
        &self.vector_parser
    }

    pub fn vector_decoder(&self) -> &str {
        &self.vector_decoder
    }

    pub fn runtime_primitive(&self) -> Option<primitive::Runtime> {
        self.slot.primitive()
    }

    fn from_native_slot(slot: direct::NativeSlot) -> Result<Self> {
        let stem = slot.stem();
        let vector_parser = match slot.primitive() {
            Some(primitive) => primitive.direct_vec_parser()?,
            None => format!("boltffi_python_parse_vec_{stem}"),
        };
        let vector_decoder = match slot.primitive() {
            Some(primitive) => primitive.direct_vec_decoder()?,
            None => format!("boltffi_python_decode_owned_vec_{stem}"),
        };
        Ok(Self {
            vector_boxer: format!("boltffi_python_box_vec_{stem}"),
            vector_encoder: format!("boltffi_python_wire_vec_{stem}"),
            vector_parser,
            vector_decoder,
            slot,
        })
    }
}

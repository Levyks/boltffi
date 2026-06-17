use boltffi_binding::{EnumId, Native, RecordId, TypeRef};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, RenderContext, Result},
    target::python::cpython::render::{enumeration, primitive, record},
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Element {
    primitive: Option<primitive::Runtime>,
    c_type: String,
    parser: String,
    boxer: String,
    vector_parser: String,
    vector_decoder: String,
}

impl Element {
    pub fn from_type_ref(
        ty: &TypeRef,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match ty {
            TypeRef::Primitive(primitive) => Self::primitive(primitive::Runtime::new(*primitive)),
            TypeRef::Record(record) => Self::record(*record, bridge, context),
            TypeRef::Enum(enumeration) => Self::enumeration(*enumeration, bridge, context),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported direct vector element",
            }),
        }
    }

    pub fn primitive(runtime: primitive::Runtime) -> Result<Self> {
        Ok(Self {
            primitive: Some(runtime),
            c_type: runtime.c_type()?,
            parser: runtime.parser()?.to_owned(),
            boxer: runtime.boxer()?.to_owned(),
            vector_parser: runtime.direct_vec_parser()?,
            vector_decoder: runtime.direct_vec_decoder()?,
        })
    }

    pub fn c_type(&self) -> &str {
        &self.c_type
    }

    pub fn parser(&self) -> &str {
        &self.parser
    }

    pub fn boxer(&self) -> &str {
        &self.boxer
    }

    pub fn vector_parser(&self) -> &str {
        &self.vector_parser
    }

    pub fn vector_decoder(&self) -> &str {
        &self.vector_decoder
    }

    pub fn runtime_primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    fn enumeration(
        enum_id: EnumId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = enumeration::Symbols::from_enum_id(enum_id, bridge, context)?;
        Ok(Self {
            primitive: None,
            c_type: symbols.c_type()?.to_owned(),
            parser: symbols.parser().to_owned(),
            boxer: symbols.boxer().to_owned(),
            vector_parser: symbols.direct_vec_parser()?.to_owned(),
            vector_decoder: symbols.direct_vec_decoder()?.to_owned(),
        })
    }

    fn record(
        record_id: RecordId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = record::Symbols::from_record_id(record_id, bridge, context)?;
        Ok(Self {
            primitive: None,
            c_type: symbols.c_type()?.to_owned(),
            parser: symbols.parser().to_owned(),
            boxer: symbols.boxer().to_owned(),
            vector_parser: symbols.direct_vec_parser()?.to_owned(),
            vector_decoder: symbols.direct_vec_decoder()?.to_owned(),
        })
    }
}

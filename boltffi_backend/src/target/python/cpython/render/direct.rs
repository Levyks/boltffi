use boltffi_binding::{EnumId, Native, Primitive, RecordId, TypeRef};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, RenderContext, Result},
    target::python::cpython::render::{enumeration, primitive, record},
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeSlot {
    stem: String,
    c_type: String,
    parser: String,
    boxer: String,
    default_value: String,
    primitive: Option<primitive::Runtime>,
}

impl NativeSlot {
    pub fn from_type_ref(
        ty: &TypeRef,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
        unsupported_shape: &'static str,
    ) -> Result<Self> {
        match ty {
            TypeRef::Primitive(primitive) => Self::from_primitive(*primitive),
            TypeRef::Record(record) => Self::from_record_id(*record, bridge, context),
            TypeRef::Enum(enumeration) => Self::from_enum_id(*enumeration, bridge, context),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: unsupported_shape,
            }),
        }
    }

    pub fn from_primitive(primitive: Primitive) -> Result<Self> {
        let runtime = primitive::Runtime::new(primitive);
        Ok(Self {
            stem: runtime.wire_stem()?.to_owned(),
            c_type: runtime.c_type()?,
            parser: runtime.parser()?.to_owned(),
            boxer: runtime.boxer()?.to_owned(),
            default_value: match primitive {
                Primitive::Bool => "false",
                Primitive::F32 => "0.0f",
                Primitive::F64 => "0.0",
                _ => "0",
            }
            .to_owned(),
            primitive: Some(runtime),
        })
    }

    pub fn from_record_id(
        record: RecordId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = record::Symbols::from_record_id(record, bridge, context)?;
        Ok(Self {
            stem: symbols.stem().to_owned(),
            c_type: symbols.c_type()?.to_owned(),
            parser: symbols.parser().to_owned(),
            boxer: symbols.boxer().to_owned(),
            default_value: "{0}".to_owned(),
            primitive: None,
        })
    }

    pub fn from_enum_id(
        enumeration: EnumId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = enumeration::Symbols::from_enum_id(enumeration, bridge, context)?;
        Ok(Self {
            stem: symbols.stem().to_owned(),
            c_type: symbols.c_type()?.to_owned(),
            parser: symbols.parser().to_owned(),
            boxer: symbols.boxer().to_owned(),
            default_value: "0".to_owned(),
            primitive: None,
        })
    }

    pub fn c_type(&self) -> &str {
        &self.c_type
    }

    pub fn stem(&self) -> &str {
        &self.stem
    }

    pub fn parser(&self) -> &str {
        &self.parser
    }

    pub fn boxer(&self) -> &str {
        &self.boxer
    }

    pub fn default_value(&self) -> &str {
        &self.default_value
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    pub fn box_expression(&self, value: &str) -> String {
        format!("{}({value})", self.boxer)
    }
}

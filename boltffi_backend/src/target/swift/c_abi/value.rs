use boltffi_binding::{DirectValueType, Native};

use crate::{
    bridge::c::CBridgeContract,
    core::{RenderContext, Result},
    target::swift::{
        SwiftHost,
        render::SwiftType,
        syntax::{ArgumentList, Expression, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectValue {
    api_type: TypeName,
    storage_type: TypeName,
    conversion: Conversion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Conversion {
    Identity,
    CValue,
}

impl DirectValue {
    pub fn new(
        ty: &DirectValueType,
        bridge: &CBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match ty {
            DirectValueType::Primitive(primitive) => {
                let ty = SwiftType::primitive(*primitive)?;
                Ok(Self {
                    api_type: ty.clone(),
                    storage_type: ty,
                    conversion: Conversion::Identity,
                })
            }
            DirectValueType::Record(record) => Ok(Self {
                api_type: SwiftType::record(*record, context)?,
                storage_type: SwiftType::direct_record_storage(*record, bridge)?,
                conversion: Conversion::CValue,
            }),
            DirectValueType::Enum(enumeration) => Ok(Self {
                api_type: SwiftType::enumeration(*enumeration, context)?,
                storage_type: SwiftType::c_style_enum_storage(*enumeration, bridge)?,
                conversion: Conversion::CValue,
            }),
            _ => Err(SwiftHost::unsupported("unknown direct value")),
        }
    }

    pub fn api_type(&self) -> &TypeName {
        &self.api_type
    }

    pub fn storage_type(&self) -> &TypeName {
        &self.storage_type
    }

    pub fn swift_value(&self, value: Expression) -> Expression {
        match self.conversion {
            Conversion::Identity => value,
            Conversion::CValue => Expression::call(
                &self.api_type,
                [Expression::labeled("fromC", value)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ),
        }
    }

    pub fn c_value(&self, value: Expression) -> Expression {
        match self.conversion {
            Conversion::Identity => value,
            Conversion::CValue => Expression::member(value, "cValue"),
        }
    }

    pub fn default_storage_value(&self) -> Expression {
        match self.conversion {
            Conversion::Identity => Expression::new("0"),
            Conversion::CValue => Expression::call(&self.storage_type, ArgumentList::default()),
        }
    }

    pub fn converts_from_c(&self) -> bool {
        matches!(self.conversion, Conversion::CValue)
    }
}

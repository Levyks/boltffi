use boltffi_binding::Primitive;

use crate::{
    core::{Error, Result},
    target::kotlin::syntax::{Expression, Identifier, TypeName},
};

pub struct KotlinPrimitive {
    primitive: Primitive,
}

impl KotlinPrimitive {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }

    pub fn api_type(self) -> Result<TypeName> {
        Ok(match self.primitive {
            Primitive::Bool => TypeName::boolean(),
            Primitive::I8 => TypeName::byte(),
            Primitive::U8 => TypeName::ubyte(),
            Primitive::I16 => TypeName::short(),
            Primitive::U16 => TypeName::ushort(),
            Primitive::I32 => TypeName::int(),
            Primitive::U32 => TypeName::uint(),
            Primitive::I64 | Primitive::ISize => TypeName::long(),
            Primitive::U64 | Primitive::USize => TypeName::ulong(),
            Primitive::F32 => TypeName::float(),
            Primitive::F64 => TypeName::double(),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "kotlin",
                    shape: "unknown primitive type",
                });
            }
        })
    }

    pub fn native_argument(self, value: Expression) -> Result<Expression> {
        Ok(
            match self.conversion("toByte", "toShort", "toInt", "toLong")? {
                Some(method) => value.convert(method),
                None => value,
            },
        )
    }

    pub fn public_return(self, value: Expression) -> Result<Expression> {
        Ok(
            match self.conversion("toUByte", "toUShort", "toUInt", "toULong")? {
                Some(method) => value.convert(method),
                None => value,
            },
        )
    }

    fn conversion(
        self,
        u8_method: &'static str,
        u16_method: &'static str,
        u32_method: &'static str,
        u64_method: &'static str,
    ) -> Result<Option<Identifier>> {
        match self.primitive {
            Primitive::U8 => Some(Identifier::parse(u8_method)),
            Primitive::U16 => Some(Identifier::parse(u16_method)),
            Primitive::U32 => Some(Identifier::parse(u32_method)),
            Primitive::U64 | Primitive::USize => Some(Identifier::parse(u64_method)),
            Primitive::Bool
            | Primitive::I8
            | Primitive::I16
            | Primitive::I32
            | Primitive::I64
            | Primitive::ISize
            | Primitive::F32
            | Primitive::F64 => None,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "kotlin",
                    shape: "unknown primitive conversion",
                });
            }
        }
        .transpose()
    }
}

use boltffi_binding::{IntegerValue, Primitive};

use crate::{
    core::{Error, Result},
    target::kotlin::syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
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

    pub fn integer_literal(self, value: IntegerValue) -> Result<Expression> {
        let value = Expression::integer(value.get());
        Ok(match self.primitive {
            Primitive::I8 => value.convert(Identifier::parse("toByte")?),
            Primitive::U8 => value.convert(Identifier::parse("toUByte")?),
            Primitive::I16 => value.convert(Identifier::parse("toShort")?),
            Primitive::U16 => value.convert(Identifier::parse("toUShort")?),
            Primitive::U32 => value.convert(Identifier::parse("toUInt")?),
            Primitive::U64 | Primitive::USize => value.convert(Identifier::parse("toULong")?),
            Primitive::I32 | Primitive::I64 | Primitive::ISize => value,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "kotlin",
                    shape: "unknown primitive literal",
                });
            }
        })
    }

    pub fn buffer_read(self, buffer: &Identifier, offset: u64) -> Result<Expression> {
        let offset = Expression::integer(offset);
        match self.primitive {
            Primitive::Bool => Ok(Self::buffer_call(buffer, "get", [offset])?
                .not_equal(Expression::integer(0).convert(Identifier::parse("toByte")?))),
            Primitive::I8 => Self::buffer_call(buffer, "get", [offset]),
            Primitive::U8 => Self::buffer_call(buffer, "get", [offset])
                .and_then(|value| Self::converted(value, "toUByte")),
            Primitive::I16 => Self::buffer_call(buffer, "getShort", [offset]),
            Primitive::U16 => Self::buffer_call(buffer, "getShort", [offset])
                .and_then(|value| Self::converted(value, "toUShort")),
            Primitive::I32 => Self::buffer_call(buffer, "getInt", [offset]),
            Primitive::U32 => Self::buffer_call(buffer, "getInt", [offset])
                .and_then(|value| Self::converted(value, "toUInt")),
            Primitive::I64 => Self::buffer_call(buffer, "getLong", [offset]),
            Primitive::U64 => Self::buffer_call(buffer, "getLong", [offset])
                .and_then(|value| Self::converted(value, "toULong")),
            Primitive::F32 => Self::buffer_call(buffer, "getFloat", [offset]),
            Primitive::F64 => Self::buffer_call(buffer, "getDouble", [offset]),
            _ => Err(Error::UnsupportedTarget {
                target: "kotlin",
                shape: "unknown direct record field read",
            }),
        }
    }

    pub fn buffer_write(
        self,
        buffer: &Identifier,
        offset: u64,
        value: Expression,
    ) -> Result<Statement> {
        let offset = Expression::integer(offset);
        let (method, value) = match self.primitive {
            Primitive::Bool => (
                "put",
                Expression::conditional(
                    value,
                    Expression::integer(1).convert(Identifier::parse("toByte")?),
                    Expression::integer(0).convert(Identifier::parse("toByte")?),
                ),
            ),
            Primitive::I8 => ("put", value),
            Primitive::U8 => ("put", value.convert(Identifier::parse("toByte")?)),
            Primitive::I16 => ("putShort", value),
            Primitive::U16 => ("putShort", value.convert(Identifier::parse("toShort")?)),
            Primitive::I32 => ("putInt", value),
            Primitive::U32 => ("putInt", value.convert(Identifier::parse("toInt")?)),
            Primitive::I64 => ("putLong", value),
            Primitive::U64 => ("putLong", value.convert(Identifier::parse("toLong")?)),
            Primitive::F32 => ("putFloat", value),
            Primitive::F64 => ("putDouble", value),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "kotlin",
                    shape: "unknown direct record field write",
                });
            }
        };
        Self::buffer_call(buffer, method, [offset, value]).map(Statement::expression)
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

    fn buffer_call(
        buffer: &Identifier,
        method: &'static str,
        arguments: impl IntoIterator<Item = Expression>,
    ) -> Result<Expression> {
        Ok(Expression::call(
            Expression::identifier(buffer.clone()),
            Identifier::parse(method)?,
            arguments.into_iter().collect::<ArgumentList>(),
        ))
    }

    fn converted(value: Expression, method: &'static str) -> Result<Expression> {
        Identifier::parse(method).map(|method| value.convert(method))
    }
}

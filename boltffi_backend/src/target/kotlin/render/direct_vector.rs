use boltffi_binding::{DirectVectorElementType, DirectVectorPrimitive, Primitive};

use crate::{
    core::{Error, Result},
    target::kotlin::{
        KotlinHost,
        primitive::KotlinPrimitive,
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectVector {
    ty: TypeName,
    decoder: Identifier,
    encoder: Identifier,
}

impl DirectVector {
    pub fn from_element(element: &DirectVectorElementType) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Self::from_primitive(*primitive),
            DirectVectorElementType::Record(_) => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "direct-record vector type",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown direct-vector type",
            }),
        }
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn value_statements(&self, call: Expression) -> Result<Vec<Statement>> {
        let result = Identifier::parse("__boltffi_result")?;
        let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
            "null buffer returned",
        )));
        let decoded = Expression::call(
            "DirectVectorCodec",
            self.decoder.clone(),
            [Expression::identifier(result.clone())]
                .into_iter()
                .collect::<ArgumentList>(),
        );
        Ok(vec![
            Statement::value(result, payload),
            Statement::expression(decoded),
        ])
    }

    pub fn byte_array_expression(&self, value: Expression) -> Expression {
        Expression::call(
            "DirectVectorCodec",
            self.encoder.clone(),
            [value].into_iter().collect::<ArgumentList>(),
        )
    }

    fn from_primitive(primitive: DirectVectorPrimitive) -> Result<Self> {
        let primitive = primitive.primitive();
        Ok(Self {
            ty: KotlinPrimitive::new(primitive).direct_vector_type()?,
            decoder: Identifier::parse(Self::decoder_name(primitive)?)?,
            encoder: Identifier::parse(Self::encoder_name(primitive)?)?,
        })
    }

    fn decoder_name(primitive: Primitive) -> Result<&'static str> {
        match primitive {
            Primitive::Bool => Ok("readBooleanArray"),
            Primitive::I8 => Ok("readByteArray"),
            Primitive::I16 => Ok("readShortArray"),
            Primitive::I32 => Ok("readIntArray"),
            Primitive::I64 | Primitive::ISize => Ok("readLongArray"),
            Primitive::F32 => Ok("readFloatArray"),
            Primitive::F64 => Ok("readDoubleArray"),
            Primitive::U8 => Ok("readByteArray"),
            Primitive::U16 => Ok("readShortArray"),
            Primitive::U32 => Ok("readIntArray"),
            Primitive::U64 | Primitive::USize => Ok("readLongArray"),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown direct-vector primitive",
            }),
        }
    }

    fn encoder_name(primitive: Primitive) -> Result<&'static str> {
        match primitive {
            Primitive::Bool => Ok("writeBooleanArray"),
            Primitive::I8 => Ok("writeByteArray"),
            Primitive::I16 | Primitive::U16 => Ok("writeShortArray"),
            Primitive::I32 | Primitive::U32 => Ok("writeIntArray"),
            Primitive::I64 | Primitive::U64 | Primitive::ISize | Primitive::USize => {
                Ok("writeLongArray")
            }
            Primitive::F32 => Ok("writeFloatArray"),
            Primitive::F64 => Ok("writeDoubleArray"),
            Primitive::U8 => Ok("writeByteArray"),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown direct-vector primitive",
            }),
        }
    }
}

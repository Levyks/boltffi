use boltffi_binding::{DefaultValue, FloatValue, Primitive, TypeRef};

use crate::{
    core::Result,
    target::swift::{
        SwiftHost,
        name_style::Name,
        syntax::{ArgumentList, Expression, Literal},
    },
};

pub struct DefaultExpression;

impl DefaultExpression {
    pub fn render(ty: &TypeRef, value: &DefaultValue) -> Result<Expression> {
        match value {
            DefaultValue::Bool(value) => Ok(Expression::literal(Literal::bool(*value))),
            DefaultValue::Integer(value) => Self::integer(ty, value.get()),
            DefaultValue::Float(value) => Self::float(ty, *value),
            DefaultValue::String(value) => Ok(Expression::literal(Literal::string(value))),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => Ok(Expression::member(
                Name::new(enum_name).type_name(),
                Name::new(variant_name).variant()?,
            )),
            DefaultValue::Null => Ok(Expression::literal(Literal::nil())),
            _ => Err(SwiftHost::unsupported("unknown default literal")),
        }
    }

    fn integer(ty: &TypeRef, value: i128) -> Result<Expression> {
        match ty {
            TypeRef::Primitive(_) => Ok(Expression::literal(Literal::integer(value))),
            _ => Err(SwiftHost::unsupported("integer default type")),
        }
    }

    fn float(ty: &TypeRef, value: FloatValue) -> Result<Expression> {
        match ty {
            TypeRef::Primitive(Primitive::F32) => Ok(Expression::call(
                "Float",
                [Expression::labeled(
                    "bitPattern",
                    Expression::new(format!("0x{:08X}", (value.to_f64() as f32).to_bits())),
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            )),
            TypeRef::Primitive(Primitive::F64) => Ok(Expression::call(
                "Double",
                [Expression::labeled(
                    "bitPattern",
                    Expression::new(format!("0x{:016X}", value.bits())),
                )]
                .into_iter()
                .collect::<ArgumentList>(),
            )),
            _ => Err(SwiftHost::unsupported("float default type")),
        }
    }
}

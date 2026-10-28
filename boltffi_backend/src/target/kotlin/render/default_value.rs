use boltffi_binding::{CustomTypeId, DefaultValue, EnumDecl, Native, Primitive, TypeRef};

use crate::{
    core::{
        RenderContext, Result,
        default_value::{Field as RepresentationField, Representation},
    },
    target::kotlin::{
        KotlinHost,
        name_style::Name,
        primitive::KotlinPrimitive,
        syntax::{ArgumentList, Expression, Literal},
    },
};

pub struct DefaultExpression;

impl DefaultExpression {
    pub fn render(
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        if let TypeRef::Custom(custom_type) = ty {
            return Self::custom(*custom_type, value, context);
        }
        // a present value of an `Option<T>` is spelled as the `T` it holds
        if let (TypeRef::Optional(inner), false) = (ty, matches!(value, DefaultValue::Null)) {
            return Self::render(inner, value, context);
        }
        if let TypeRef::Primitive(primitive) = ty {
            return Self::primitive(*primitive, value);
        }

        match value {
            DefaultValue::Bool(_) | DefaultValue::Integer(_) | DefaultValue::Float(_) => Err(
                KotlinHost::unsupported("default value requires a primitive type"),
            ),
            DefaultValue::String(value) => Ok(Expression::literal(Literal::string(value))),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => match ty {
                TypeRef::Enum(id) => context
                    .enumeration(*id)
                    .ok_or(KotlinHost::broken_bridge_contract(
                        "enum default type was not found",
                    ))
                    .and_then(|enumeration| {
                        let variant = match enumeration {
                            EnumDecl::CStyle(_) => Name::new(variant_name).enum_entry()?,
                            EnumDecl::Data(_) => Name::new(variant_name).variant()?,
                            _ => return Err(KotlinHost::unsupported("enum default declaration")),
                        };
                        Ok(Expression::property(
                            Name::new(enum_name).type_name(),
                            variant,
                        ))
                    }),
                _ => Err(KotlinHost::unsupported("enum default type")),
            },
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(KotlinHost::unsupported("unknown default literal")),
        }
    }

    pub fn primitive(primitive: Primitive, value: &DefaultValue) -> Result<Expression> {
        match (primitive, value) {
            (Primitive::Bool, DefaultValue::Bool(value)) => Ok(Expression::bool(*value)),
            (_, DefaultValue::Integer(value)) => {
                KotlinPrimitive::new(primitive).integer_literal(*value)
            }
            (Primitive::F32, DefaultValue::Float(value)) => {
                Ok(Expression::float(value.to_f64(), true))
            }
            (Primitive::F64, DefaultValue::Float(value)) => {
                Ok(Expression::float(value.to_f64(), false))
            }
            _ => Err(KotlinHost::unsupported(
                "default value does not match its primitive type",
            )),
        }
    }

    fn custom(
        custom_type: CustomTypeId,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let representation = match Representation::resolve_ffi(custom_type, context)? {
            Representation::Transparent(representation) => {
                Self::render(representation, value, context)
            }
            Representation::Record(record) => {
                let value = match record.field() {
                    RepresentationField::Direct(field) => {
                        Self::render(&TypeRef::Primitive(field.ty().primitive()), value, context)?
                    }
                    RepresentationField::Encoded(field) => {
                        Self::render(field.ty(), value, context)?
                    }
                };
                Ok(Expression::construct(
                    Name::new(record.name()).type_name(),
                    [value].into_iter().collect::<ArgumentList>(),
                ))
            }
        }?;
        match context.custom_type_mapping(custom_type) {
            Some(mapping) => KotlinHost::custom_type_decode(mapping, representation),
            None => Ok(representation),
        }
    }
}

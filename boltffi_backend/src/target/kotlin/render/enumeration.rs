use askama::Template as AskamaTemplate;
use boltffi_binding::{CStyleEnumDecl, CStyleVariantDecl, EnumDecl, EnumId, Native, Primitive};

use crate::{
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        name_style::Name,
        render::primitive::KotlinPrimitive,
        syntax::{Expression, Identifier, TypeName},
    },
};

const KOTLIN_TARGET: &str = "kotlin";

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/enumeration.kt", escape = "none")]
struct EnumerationTemplate {
    enumeration: Enumeration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enumeration {
    name: TypeName,
    value_type: TypeName,
    repr: Primitive,
    variants: Vec<Variant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Variant {
    name: Identifier,
    value: Expression,
}

impl Enumeration {
    pub fn from_declaration(declaration: &EnumDecl<Native>) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration),
            EnumDecl::Data(_) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "data enum declaration",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown enum declaration",
            }),
        }
    }

    pub fn from_id(id: EnumId, context: &RenderContext<Native>) -> Result<Self> {
        context
            .enumeration(id)
            .ok_or(Error::BrokenBridgeContract {
                bridge: KOTLIN_TARGET,
                invariant: "enum type was not found in render context",
            })
            .and_then(Self::from_declaration)
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            EnumerationTemplate { enumeration: self }.render()?,
        ))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn value_type(&self) -> &TypeName {
        &self.value_type
    }

    pub fn repr(&self) -> Primitive {
        self.repr
    }

    pub fn variants(&self) -> &[Variant] {
        &self.variants
    }

    pub fn type_name_from_id(id: EnumId, context: &RenderContext<Native>) -> Result<TypeName> {
        Self::from_id(id, context).map(|enumeration| enumeration.name)
    }

    pub fn native_argument(
        id: EnumId,
        value: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let enumeration = Self::from_id(id, context)?;
        KotlinPrimitive::new(enumeration.repr)
            .native_argument(Expression::property(value, Identifier::parse("value")?))
    }

    fn from_c_style(enumeration: &CStyleEnumDecl<Native>) -> Result<Self> {
        let primitive = KotlinPrimitive::new(enumeration.repr().primitive());
        Ok(Self {
            name: Name::new(enumeration.name()).type_name(),
            value_type: primitive.api_type()?,
            repr: enumeration.repr().primitive(),
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| Variant::from_c_style(variant, enumeration))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl Variant {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }

    fn from_c_style(
        variant: &CStyleVariantDecl,
        enumeration: &CStyleEnumDecl<Native>,
    ) -> Result<Self> {
        Ok(Self {
            name: Name::new(variant.name()).variant()?,
            value: KotlinPrimitive::new(enumeration.repr().primitive())
                .integer_literal(variant.discriminant())?,
        })
    }
}

use askama::Template;
use boltffi_binding::{CStyleEnumDecl, CStyleVariantDecl, EnumDecl, Native};

use crate::{
    core::{Emitted, Result},
    target::swift::{
        SwiftHost,
        name_style::Name,
        render::{Documentation, SwiftType},
        syntax::{Identifier, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/enumeration.swift", escape = "none")]
struct EnumerationTemplate<'a> {
    enumeration: &'a Enumeration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enumeration {
    documentation: Documentation,
    name: TypeName,
    raw_type: TypeName,
    variants: Vec<Variant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Variant {
    documentation: Documentation,
    name: Identifier,
    discriminant: i128,
}

impl Enumeration {
    pub fn from_declaration(declaration: &EnumDecl<Native>) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration),
            EnumDecl::Data(_) => Err(SwiftHost::unsupported("data enum declaration")),
            _ => Err(SwiftHost::unsupported("unknown enum declaration")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = EnumerationTemplate { enumeration: self }.render()?;
        source.push_str("\n\n");
        Ok(Emitted::primary(source))
    }

    fn name(&self) -> &TypeName {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn raw_type(&self) -> &TypeName {
        &self.raw_type
    }

    fn variants(&self) -> &[Variant] {
        &self.variants
    }

    fn from_c_style(enumeration: &CStyleEnumDecl<Native>) -> Result<Self> {
        Ok(Self {
            documentation: Documentation::new(enumeration.meta().doc(), ""),
            name: Name::new(enumeration.name()).type_name(),
            raw_type: SwiftType::primitive(enumeration.repr().primitive())?,
            variants: enumeration
                .variants()
                .iter()
                .map(Variant::from_declaration)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl Variant {
    fn from_declaration(variant: &CStyleVariantDecl) -> Result<Self> {
        Ok(Self {
            documentation: Documentation::new(variant.meta().doc(), "    "),
            name: Name::new(variant.name()).variant()?,
            discriminant: variant.discriminant().get(),
        })
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    const fn discriminant(&self) -> i128 {
        self.discriminant
    }
}

use askama::Template;
use boltffi_binding::{DirectFieldDecl, DirectRecordDecl, FieldKey, Native, RecordDecl};

use crate::{
    bridge::c::CBridgeContract,
    core::{Emitted, Error, Result},
    target::swift::{
        SwiftHost,
        name_style::Name,
        render::{Documentation, SwiftType},
        syntax::{Expression, Identifier, TypeName},
    },
};

#[derive(Template)]
#[template(path = "target/swift/record.swift", escape = "none")]
struct RecordTemplate<'a> {
    record: &'a Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    documentation: Documentation,
    name: TypeName,
    c_type: TypeName,
    fields: Vec<Field>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    documentation: Documentation,
    name: Identifier,
    c_name: Identifier,
    ty: TypeName,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &CBridgeContract,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, bridge),
            RecordDecl::Encoded(_) => Err(SwiftHost::unsupported("encoded record declaration")),
            _ => Err(SwiftHost::unsupported("unknown record declaration")),
        }
    }

    pub fn render(&self) -> Result<Emitted> {
        let mut source = RecordTemplate { record: self }.render()?;
        source.push_str("\n\n");
        Ok(Emitted::primary(source))
    }

    fn name(&self) -> &TypeName {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn c_type(&self) -> &TypeName {
        &self.c_type
    }

    fn fields(&self) -> &[Field] {
        &self.fields
    }

    fn from_direct(record: &DirectRecordDecl<Native>, bridge: &CBridgeContract) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(record.id())
                .ok_or(Error::BrokenBridgeContract {
                    bridge: SwiftHost::TARGET,
                    invariant: "missing direct record C type for Swift record",
                })?;
        if record.fields().len() != c_record.fields().len() {
            return Err(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "direct record field count mismatch",
            });
        }
        Ok(Self {
            documentation: Documentation::new(record.meta().doc(), ""),
            name: Name::new(record.name()).type_name(),
            c_type: TypeName::new(c_record.name()),
            fields: record
                .fields()
                .iter()
                .zip(c_record.fields())
                .map(|(field, c_field)| Field::from_direct(field, c_field.name()))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl Field {
    fn from_direct(field: &DirectFieldDecl, c_name: &str) -> Result<Self> {
        Ok(Self {
            documentation: Documentation::new(field.meta().doc(), "    "),
            name: Self::field_name(field.key())?,
            c_name: Identifier::parse(c_name)?,
            ty: SwiftType::primitive(field.ty().primitive())?,
        })
    }

    fn name(&self) -> &Identifier {
        &self.name
    }

    fn documentation(&self) -> &Documentation {
        &self.documentation
    }

    fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn assignment(&self) -> Expression {
        Expression::new(format!("self.{} = {}", self.name, self.name))
    }

    fn c_initializer_argument(&self) -> Expression {
        Expression::labeled(&self.name, Expression::member("c", &self.c_name))
    }

    fn c_value_argument(&self) -> Expression {
        Expression::labeled(&self.c_name, &self.name)
    }

    fn field_name(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).field(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(SwiftHost::unsupported("unknown direct record field key")),
        }
    }
}

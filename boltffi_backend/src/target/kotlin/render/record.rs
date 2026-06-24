use askama::Template as AskamaTemplate;
use boltffi_binding::{DirectFieldDecl, DirectRecordDecl, FieldKey, Native, RecordDecl, RecordId};

use crate::{
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        name_style::Name,
        render::primitive::KotlinPrimitive,
        syntax::{Expression, Identifier, Statement, TypeName},
    },
};

const KOTLIN_TARGET: &str = "kotlin";

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/record.kt", escape = "none")]
struct RecordTemplate {
    record: Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    name: TypeName,
    size: u64,
    fields: Vec<Field>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    name: Identifier,
    ty: TypeName,
    read: Expression,
    write: Statement,
}

impl Record {
    pub fn from_declaration(declaration: &RecordDecl<Native>) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record),
            RecordDecl::Encoded(_) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "encoded record declaration",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown record declaration",
            }),
        }
    }

    pub fn from_id(id: RecordId, context: &RenderContext<Native>) -> Result<Self> {
        context
            .record(id)
            .ok_or(Error::BrokenBridgeContract {
                bridge: KOTLIN_TARGET,
                invariant: "record type was not found in render context",
            })
            .and_then(Self::from_declaration)
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(RecordTemplate { record: self }.render()?))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn type_name_from_id(id: RecordId, context: &RenderContext<Native>) -> Result<TypeName> {
        Self::from_id(id, context).map(|record| record.name)
    }

    pub fn encode_expression(value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            value,
            Identifier::parse("toByteArray")?,
            Default::default(),
        ))
    }

    pub fn decode_expression(record: TypeName, value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            record,
            Identifier::parse("fromByteArray")?,
            [value].into_iter().collect(),
        ))
    }

    fn from_direct(record: &DirectRecordDecl<Native>) -> Result<Self> {
        let buffer = Identifier::parse("buffer")?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            size: record.layout().size().get(),
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_direct(field, record, &buffer))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl Field {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn read(&self) -> &Expression {
        &self.read
    }

    pub fn write(&self) -> &Statement {
        &self.write
    }

    fn from_direct(
        field: &DirectFieldDecl,
        record: &DirectRecordDecl<Native>,
        buffer: &Identifier,
    ) -> Result<Self> {
        let name = Self::identifier(field.key())?;
        let offset = record
            .layout()
            .field(field.key())
            .ok_or(Error::BrokenBridgeContract {
                bridge: KOTLIN_TARGET,
                invariant: "direct record field layout was not found",
            })?
            .offset()
            .get();
        let primitive = field.ty().primitive();
        Ok(Self {
            ty: KotlinPrimitive::new(primitive).api_type()?,
            read: KotlinPrimitive::new(primitive).buffer_read(buffer, offset)?,
            write: KotlinPrimitive::new(primitive).buffer_write(
                buffer,
                offset,
                Expression::identifier(name.clone()),
            )?,
            name,
        })
    }

    fn identifier(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).parameter(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown direct record field key",
            }),
        }
    }
}

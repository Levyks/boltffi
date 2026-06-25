use boltffi_binding::{EncodedFieldDecl, FieldKey, Native};

use crate::{
    core::{Error, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::{Reader, Sizer, Writer},
        name_style::KotlinPackage,
        name_style::Name,
        render::type_name::KotlinType,
        syntax::{Expression, Identifier, Statement, TypeName},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedField {
    name: Identifier,
    ty: TypeName,
    read: Expression,
    write: Statement,
    size: Expression,
}

impl EncodedField {
    pub fn from_declaration(
        field: &EncodedFieldDecl,
        context: &RenderContext<Native>,
        reader: &Identifier,
        writer: &Identifier,
        current: Expression,
    ) -> Result<Self> {
        Self::from_declaration_with_reader(
            field,
            context,
            Reader::new(reader.clone(), context),
            KotlinType::type_ref(field.ty(), context)?,
            writer,
            current,
        )
    }

    pub fn from_enum_payload(
        field: &EncodedFieldDecl,
        context: &RenderContext<Native>,
        reader: &Identifier,
        writer: &Identifier,
        current: Expression,
        package: &KotlinPackage,
    ) -> Result<Self> {
        Self::from_declaration_with_reader(
            field,
            context,
            Reader::new(reader.clone(), context).record_package(package),
            KotlinType::type_ref_with_record_package(field.ty(), context, package)?,
            writer,
            current,
        )
    }

    fn from_declaration_with_reader(
        field: &EncodedFieldDecl,
        context: &RenderContext<Native>,
        mut reader: Reader,
        ty: TypeName,
        writer: &Identifier,
        current: Expression,
    ) -> Result<Self> {
        let mut writer = Writer::new(writer.clone(), context)?.current(current.clone());
        let write = field
            .write()
            .render_with(&mut writer)
            .into_iter()
            .map(|write| write.map(|write| write.into_statement()))
            .collect::<Result<Vec<_>>>()?;
        match write.as_slice() {
            [write] => Ok(Self {
                name: Self::identifier(field.key())?,
                ty,
                read: field.read().render_with(&mut reader)?.into_expression(),
                write: write.clone(),
                size: field
                    .write()
                    .size_with(&mut Sizer::new(context)?.current(current))?
                    .into_expression(),
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "multi-statement encoded field",
            }),
        }
    }

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

    pub fn size(&self) -> &Expression {
        &self.size
    }

    fn identifier(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).parameter(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown encoded field key",
            }),
        }
    }
}

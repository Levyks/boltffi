use boltffi_binding::{
    DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, ExportedMethodDecl,
    FieldKey, InitializerDecl, Native, NativeSymbol, Receive,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::Expression as CodecExpression,
        cpython::render::{function, record as record_render},
        name_style::Name,
        render::Package,
        syntax::{Expression, Identifier, TypeAnnotation},
    },
};

use super::{AssociatedCallable, NameScope, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordClass {
    pub class_name: Identifier,
    pub register_method: Identifier,
    pub fields: Vec<RecordField>,
    pub wire: Option<EncodedRecordWire>,
    pub constructors: Vec<AssociatedCallable>,
    pub static_methods: Vec<AssociatedCallable>,
    pub instance_methods: Vec<AssociatedCallable>,
}

impl RecordClass {
    pub fn from_direct<'package>(
        record: &DirectRecordDecl<Native>,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        let c_record =
            package
                .bridge
                .source_direct_record(record.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct record package without C typedef",
                })?;
        let symbols = record_render::Symbols::from_direct(record, c_record)?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            register_method: symbols.register_method().clone(),
            fields: record
                .fields()
                .iter()
                .map(RecordField::from_direct)
                .collect::<Result<Vec<_>>>()?,
            wire: None,
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    pub fn from_encoded<'package>(
        record: &EncodedRecordDecl<Native>,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        let symbols = record_render::Symbols::from_encoded(record)?;
        let fields = record
            .fields()
            .iter()
            .map(|field| RecordField::from_encoded(field, package))
            .collect::<Result<Vec<_>>>()?;
        let wire_fields = record
            .fields()
            .iter()
            .map(|field| EncodedRecordField::from_field(field, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name: symbols.class_name().clone(),
            register_method: symbols.register_method().clone(),
            fields,
            wire: Some(EncodedRecordWire {
                fields: wire_fields,
            }),
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    pub fn has_wire(&self) -> bool {
        self.wire.is_some()
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_wire_helpers)
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    pub fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    pub fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    pub fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("record `{}`", self.class_name))
            .insert_all(self.fields.iter().map(RecordField::field_name))
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .map(|_| ())?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.to_string(),
            format!("record `{}`", self.class_name),
        )
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn constructors<'package>(
        initializers: &[InitializerDecl<Native>],
        symbols: &record_render::Symbols,
        package: &'package Package<'package>,
    ) -> Result<Vec<AssociatedCallable>> {
        initializers
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_value_initializer(
                    initializer,
                    symbols.initializer(initializer.name())?,
                    package,
                )
            })
            .collect()
    }

    fn static_methods<'package>(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &'package Package<'package>,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_none()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name())?,
                    None,
                    None,
                    package,
                )
            })
            .collect()
    }

    fn instance_methods<'package>(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &'package Package<'package>,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_some()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name())?,
                    Some(Expression::identifier(Identifier::parse("self")?)),
                    method
                        .callable()
                        .receiver()
                        .filter(|receiver| matches!(receiver, Receive::ByMutRef))
                        .map(|_| TypeAnnotation::identifier(symbols.class_name().clone())),
                    package,
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordField {
    pub name: Identifier,
    pub annotation: TypeAnnotation,
}

impl RecordField {
    pub fn from_encoded<'package>(
        field: &EncodedFieldDecl,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_type_ref(field.ty(), package)?.into_annotation(),
        })
    }

    pub fn field_name(&self) -> (String, String) {
        (self.name.to_string(), format!("field `{}`", self.name))
    }

    pub fn name(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => Identifier::parse(format!("field_{position}")),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record field annotation",
            }),
        }
    }

    fn from_direct(field: &DirectFieldDecl) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_primitive(field.ty().primitive())?.into_annotation(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecordWire {
    pub fields: Vec<EncodedRecordField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecordField {
    pub name: Identifier,
    pub encode: Expression,
    pub decode: Expression,
}

impl EncodedRecordField {
    pub fn from_field<'package>(
        field: &EncodedFieldDecl,
        package: &'package Package<'package>,
    ) -> Result<Self> {
        let name = RecordField::name(field.key())?;
        Ok(Self {
            encode: CodecExpression::write(field.write(), package)?.into_expression(),
            decode: CodecExpression::read(field.read(), package)?.into_expression(),
            name,
        })
    }
}

use boltffi_binding::{
    DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, ExportedMethodDecl,
    FieldKey, InitializerDecl, Native, NativeSymbol, Receive, TypeRef,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::Expression as CodecExpression,
        cpython::render::{function, record as record_render},
        name_style::Name,
        render::Package,
    },
};

use super::{AssociatedCallable, NameScope, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordClass {
    pub class_name: String,
    pub register_method: String,
    pub fields: Vec<RecordField>,
    pub wire: Option<EncodedRecordWire>,
    pub constructors: Vec<AssociatedCallable>,
    pub static_methods: Vec<AssociatedCallable>,
    pub instance_methods: Vec<AssociatedCallable>,
}

impl RecordClass {
    pub fn from_direct(
        record: &DirectRecordDecl<Native>,
        package: &Package<'_, '_>,
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
            class_name: symbols.class_name().to_owned(),
            register_method: symbols.register_method().to_owned(),
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

    pub fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        package: &Package<'_, '_>,
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
            class_name: symbols.class_name().to_owned(),
            register_method: symbols.register_method().to_owned(),
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
            self.class_name.clone(),
            format!("record `{}`", self.class_name),
        )
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn constructors(
        initializers: &[InitializerDecl<Native>],
        symbols: &record_render::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        initializers
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_value_initializer(
                    initializer,
                    symbols.initializer(initializer.name()),
                    package,
                )
            })
            .collect()
    }

    fn static_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &Package<'_, '_>,
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
                    symbols.method(method.name()),
                    None,
                    None,
                    package,
                )
            })
            .collect()
    }

    fn instance_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record_render::Symbols,
        package: &Package<'_, '_>,
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
                    symbols.method(method.name()),
                    Some("self"),
                    method
                        .callable()
                        .receiver()
                        .filter(|receiver| matches!(receiver, Receive::ByMutRef))
                        .map(|_| symbols.class_name()),
                    package,
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecordField {
    pub name: String,
    pub annotation: String,
}

impl RecordField {
    pub fn from_encoded(field: &EncodedFieldDecl, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_type_ref(field.ty(), package)?.into_string(),
        })
    }

    pub fn field_name(&self) -> (String, String) {
        (self.name.clone(), format!("field `{}`", self.name))
    }

    pub fn name(key: &FieldKey) -> Result<String> {
        Ok(match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => format!("field_{position}"),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown record field annotation",
                });
            }
        })
    }

    fn from_direct(field: &DirectFieldDecl) -> Result<Self> {
        let TypeRef::Primitive(primitive) = field.ty() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "non-primitive record field annotation",
            });
        };
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: TypeHint::from_primitive(*primitive)?.into_string(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecordWire {
    pub fields: Vec<EncodedRecordField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedRecordField {
    pub name: String,
    pub encode: String,
    pub decode: String,
}

impl EncodedRecordField {
    pub fn from_field(field: &EncodedFieldDecl, package: &Package<'_, '_>) -> Result<Self> {
        let name = RecordField::name(field.key())?;
        Ok(Self {
            encode: CodecExpression::write(field.write(), package)?.into_string(),
            decode: CodecExpression::read(field.read(), package)?.into_string(),
            name,
        })
    }
}

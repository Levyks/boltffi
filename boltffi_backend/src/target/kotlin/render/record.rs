use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CanonicalName, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl,
    ExportedMethodDecl, FieldKey, InitializerDecl, Native, NativeSymbol, Receive, RecordDecl,
    RecordId, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::Sizer,
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            default_value::DefaultExpression,
            field::EncodedField,
            function::{EncodedReceiverMutation, ExportedCall},
        },
        syntax::{ArgumentList, Expression, Identifier, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/record.kt", escape = "none")]
struct RecordTemplate {
    record: Record,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    name: TypeName,
    body: RecordBody,
    error: bool,
    fields: Vec<Field>,
    initializers: Vec<ExportedCall>,
    static_methods: Vec<ExportedCall>,
    instance_methods: Vec<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RecordBody {
    Direct { size: u64 },
    Encoded { size: Expression },
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Receiver {
    argument: Expression,
    writeback: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    name: Identifier,
    ty: TypeName,
    read: Expression,
    read_from_base: Option<Expression>,
    write: Statement,
    size: Option<Expression>,
    default: Option<Expression>,
}

impl Record {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_declaration_with_error(declaration, bridge, context, false)
    }

    pub fn from_declaration_as_error(
        declaration: &RecordDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_declaration_with_error(declaration, bridge, context, true)
    }

    pub fn from_declaration_with_error(
        declaration: &RecordDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        error: bool,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, bridge, context, error),
            RecordDecl::Encoded(record) => Self::from_encoded(record, bridge, context, error),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown record declaration",
            }),
        }
    }

    pub fn from_id(id: RecordId, context: &RenderContext<Native>) -> Result<Self> {
        context
            .record(id)
            .ok_or(Error::BrokenBridgeContract {
                bridge: KotlinHost::TARGET,
                invariant: "record type was not found in render context",
            })
            .and_then(|record| Self::shape_from_declaration(record, context))
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(RecordTemplate { record: self }.render()?))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn size(&self) -> u64 {
        match self.body {
            RecordBody::Direct { size } => size,
            RecordBody::Encoded { .. } => 0,
        }
    }

    pub fn wire_size(&self) -> Option<&Expression> {
        match &self.body {
            RecordBody::Encoded { size } => Some(size),
            RecordBody::Direct { .. } => None,
        }
    }

    pub fn encoded(&self) -> bool {
        matches!(self.body, RecordBody::Encoded { .. })
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn error_message(&self) -> Option<&Identifier> {
        self.fields
            .iter()
            .find(|field| field.is_string_message())
            .map(|field| field.name())
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn initializers(&self) -> &[ExportedCall] {
        &self.initializers
    }

    pub fn static_methods(&self) -> &[ExportedCall] {
        &self.static_methods
    }

    pub fn instance_methods(&self) -> &[ExportedCall] {
        &self.instance_methods
    }

    pub fn direct_fields(&self) -> &[Field] {
        match self.body {
            RecordBody::Direct { .. } => &self.fields,
            RecordBody::Encoded { .. } => &[],
        }
    }

    pub fn empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn type_name_from_id(id: RecordId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .record(id)
            .ok_or(Error::BrokenBridgeContract {
                bridge: KotlinHost::TARGET,
                invariant: "record type was not found in render context",
            })
            .map(Self::name_from_declaration)
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

    fn from_direct(
        record: &DirectRecordDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        error: bool,
    ) -> Result<Self> {
        let buffer = Identifier::parse("buffer")?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::Direct {
                size: record.layout().size().get(),
            },
            error,
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_direct(field, record, &buffer))
                .collect::<Result<Vec<_>>>()?,
            initializers: Self::initializer_calls(record.initializers(), bridge, context)?,
            static_methods: Self::methods(record.methods(), None, bridge, context)?,
            instance_methods: Self::methods(
                record.methods(),
                Some(Self::receiver(record.name())?),
                bridge,
                context,
            )?,
        })
    }

    fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        error: bool,
    ) -> Result<Self> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let current = Expression::this();
        let size = record
            .fields()
            .iter()
            .map(|field| {
                field
                    .write()
                    .size_with(&mut Sizer::new(context)?.current(current.clone()))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|size| size.into_expression())
            .reduce(Expression::add)
            .unwrap_or_else(|| Expression::integer(0));
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::Encoded { size },
            error,
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_encoded(field, context, &reader, &writer, current.clone()))
                .collect::<Result<Vec<_>>>()?,
            initializers: Self::initializer_calls(record.initializers(), bridge, context)?,
            static_methods: Self::methods(record.methods(), None, bridge, context)?,
            instance_methods: Self::methods(
                record.methods(),
                Some(Self::receiver(record.name())?),
                bridge,
                context,
            )?,
        })
    }

    fn shape_from_declaration(
        declaration: &RecordDecl<Native>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::shape_from_direct(record),
            RecordDecl::Encoded(record) => Self::shape_from_encoded(record, context),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown record declaration",
            }),
        }
    }

    fn shape_from_direct(record: &DirectRecordDecl<Native>) -> Result<Self> {
        let buffer = Identifier::parse("buffer")?;
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::Direct {
                size: record.layout().size().get(),
            },
            error: false,
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_direct(field, record, &buffer))
                .collect::<Result<Vec<_>>>()?,
            initializers: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
    }

    fn shape_from_encoded(
        record: &EncodedRecordDecl<Native>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let current = Expression::this();
        let size = record
            .fields()
            .iter()
            .map(|field| {
                field
                    .write()
                    .size_with(&mut Sizer::new(context)?.current(current.clone()))
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|size| size.into_expression())
            .reduce(Expression::add)
            .unwrap_or_else(|| Expression::integer(0));
        Ok(Self {
            name: Name::new(record.name()).type_name(),
            body: RecordBody::Encoded { size },
            error: false,
            fields: record
                .fields()
                .iter()
                .map(|field| Field::from_encoded(field, context, &reader, &writer, current.clone()))
                .collect::<Result<Vec<_>>>()?,
            initializers: Vec::new(),
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
        })
    }

    fn name_from_declaration(record: &RecordDecl<Native>) -> TypeName {
        Name::new(record.name()).type_name()
    }

    fn initializer_calls(
        initializers: &[InitializerDecl<Native>],
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<ExportedCall>> {
        initializers
            .iter()
            .map(|initializer| {
                ExportedCall::new(
                    Name::new(initializer.name()).function()?,
                    initializer.symbol(),
                    initializer.callable(),
                    Vec::new(),
                    bridge,
                    context,
                )
            })
            .collect()
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<ExportedCall>> {
        methods
            .iter()
            .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
            .map(
                |method| match (method.callable().receiver(), receiver.clone()) {
                    (Some(Receive::ByMutRef), Some(receiver)) => {
                        ExportedCall::new_encoded_receiver_mutation(
                            Name::new(method.name()).function()?,
                            method.target(),
                            method.callable(),
                            vec![receiver.argument],
                            EncodedReceiverMutation::new(receiver.writeback),
                            bridge,
                            context,
                        )
                    }
                    (Some(Receive::ByRef | Receive::ByValue), Some(receiver)) => ExportedCall::new(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        vec![receiver.argument],
                        bridge,
                        context,
                    ),
                    (None, None) => ExportedCall::new(
                        Name::new(method.name()).function()?,
                        method.target(),
                        method.callable(),
                        Vec::new(),
                        bridge,
                        context,
                    ),
                    _ => Err(Error::UnsupportedTarget {
                        target: KotlinHost::TARGET,
                        shape: "record method receiver",
                    }),
                },
            )
            .collect()
    }

    fn receiver(name: &CanonicalName) -> Result<Receiver> {
        Ok(Receiver {
            argument: Expression::call(
                Expression::this(),
                Identifier::parse("toByteArray")?,
                ArgumentList::default(),
            ),
            writeback: Name::new(name).type_name(),
        })
    }
}

impl Field {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn is_string_message(&self) -> bool {
        self.name.to_string() == "message" && self.ty.to_string() == "String"
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn read(&self) -> &Expression {
        &self.read
    }

    pub fn read_from_base(&self) -> &Expression {
        self.read_from_base
            .as_ref()
            .expect("direct field has offset-based read expression")
    }

    pub fn write(&self) -> &Statement {
        &self.write
    }

    pub fn default(&self) -> Option<&Expression> {
        self.default.as_ref()
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
                bridge: KotlinHost::TARGET,
                invariant: "direct record field layout was not found",
            })?
            .offset()
            .get();
        let base = Identifier::parse("offset")?;
        let position = match offset {
            0 => Expression::identifier(base),
            _ => Expression::identifier(base).add(Expression::integer(offset)),
        };
        let primitive = field.ty().primitive();
        let default = field
            .meta()
            .default()
            .map(|value| DefaultExpression::render(&TypeRef::Primitive(primitive), value))
            .transpose()?;
        Ok(Self {
            ty: KotlinPrimitive::new(primitive).api_type()?,
            read: KotlinPrimitive::new(primitive).buffer_read(buffer, offset)?,
            read_from_base: Some(KotlinPrimitive::new(primitive).buffer_read_at(buffer, position)?),
            write: KotlinPrimitive::new(primitive).buffer_write(
                buffer,
                offset,
                Expression::identifier(name.clone()),
            )?,
            size: None,
            default,
            name,
        })
    }

    fn from_encoded(
        field: &EncodedFieldDecl,
        context: &RenderContext<Native>,
        reader: &Identifier,
        writer: &Identifier,
        current: Expression,
    ) -> Result<Self> {
        let name = Self::identifier(field.key())?;
        let default = field
            .meta()
            .default()
            .map(|value| DefaultExpression::render(field.ty(), value))
            .transpose()?;
        let field = EncodedField::from_declaration(field, context, reader, writer, current)?;
        Ok(Self {
            ty: field.ty().clone(),
            read: field.read().clone(),
            read_from_base: None,
            write: field.write().clone(),
            size: Some(field.size().clone()),
            default,
            name,
        })
    }

    fn identifier(key: &FieldKey) -> Result<Identifier> {
        match key {
            FieldKey::Named(name) => Name::new(name).parameter(),
            FieldKey::Position(position) => Identifier::parse(format!("field{position}")),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown direct record field key",
            }),
        }
    }
}

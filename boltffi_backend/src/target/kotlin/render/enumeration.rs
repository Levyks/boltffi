use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, CStyleVariantDecl, DataEnumDecl, DataVariantDecl, DataVariantPayload, EnumDecl,
    EnumId, ExportedMethodDecl, InitializerDecl, Native, NativeSymbol, Primitive, Receive,
    VariantTag,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        name_style::KotlinPackage,
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            field::EncodedField,
            function::{EncodedReceiverMutation, ExportedCall},
        },
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/enumeration.kt", escape = "none")]
struct EnumerationTemplate {
    enumeration: Enumeration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Enumeration {
    name: TypeName,
    error: bool,
    body: Body,
    initializers: Vec<ExportedCall>,
    static_methods: Vec<ExportedCall>,
    instance_methods: Vec<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Body {
    CStyle {
        value_type: TypeName,
        repr: Primitive,
        variants: Vec<CStyleVariant>,
    },
    Data {
        variants: Vec<DataVariant>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CStyleVariant {
    name: Identifier,
    value: Expression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataVariant {
    name: Identifier,
    tag: Expression,
    fields: Vec<EncodedField>,
    read: Expression,
    size: Expression,
    tag_write: Statement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Receiver {
    argument: Expression,
    writeback: Option<TypeName>,
}

impl Enumeration {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, None, context, false),
            EnumDecl::Data(enumeration) => Self::from_data(enumeration, None, context, None, false),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown enum declaration",
            }),
        }
    }

    pub fn from_declaration_with_package(
        declaration: &EnumDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        Self::from_declaration_with_package_and_error(declaration, bridge, context, package, false)
    }

    pub fn from_declaration_as_error(
        declaration: &EnumDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        Self::from_declaration_with_package_and_error(declaration, bridge, context, package, true)
    }

    pub fn from_declaration_with_package_and_error(
        declaration: &EnumDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
        error: bool,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => {
                Self::from_c_style(enumeration, Some(bridge), context, error)
            }
            EnumDecl::Data(enumeration) => {
                Self::from_data(enumeration, Some(bridge), context, package, error)
            }
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown enum declaration",
            }),
        }
    }

    pub fn from_id(id: EnumId, context: &RenderContext<Native>) -> Result<Self> {
        context
            .enumeration(id)
            .ok_or(Error::BrokenBridgeContract {
                bridge: KotlinHost::TARGET,
                invariant: "enum type was not found in render context",
            })
            .and_then(|enumeration| Self::from_declaration(enumeration, context))
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            EnumerationTemplate { enumeration: self }.render()?,
        ))
    }

    pub fn name(&self) -> &TypeName {
        &self.name
    }

    pub fn c_style(&self) -> bool {
        matches!(&self.body, Body::CStyle { .. })
    }

    pub fn error(&self) -> bool {
        self.error
    }

    pub fn data(&self) -> bool {
        matches!(&self.body, Body::Data { .. })
    }

    pub fn value_type(&self) -> Option<&TypeName> {
        match &self.body {
            Body::CStyle { value_type, .. } => Some(value_type),
            Body::Data { .. } => None,
        }
    }

    pub fn repr(&self) -> Result<Primitive> {
        match &self.body {
            Body::CStyle { repr, .. } => Ok(*repr),
            Body::Data { .. } => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "data enum has no direct repr",
            }),
        }
    }

    pub fn c_style_variants(&self) -> &[CStyleVariant] {
        match &self.body {
            Body::CStyle { variants, .. } => variants,
            Body::Data { .. } => &[],
        }
    }

    pub fn data_variants(&self) -> &[DataVariant] {
        match &self.body {
            Body::Data { variants } => variants,
            Body::CStyle { .. } => &[],
        }
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

    pub fn unknown_tag(&self) -> Expression {
        Expression::throw_illegal_argument(Literal::string(&format!("unknown {self} tag: $tag")))
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
        KotlinPrimitive::new(enumeration.repr()?)
            .native_argument(Expression::property(value, Identifier::parse("value")?))
    }

    pub fn read_expression(
        id: EnumId,
        reader: Identifier,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        Self::type_name_from_id(id, context).and_then(|enumeration| {
            Ok(Expression::call(
                enumeration,
                Identifier::parse("fromReader")?,
                [Expression::identifier(reader)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))
        })
    }

    pub fn write_statement(
        id: EnumId,
        value: Expression,
        writer: Identifier,
        context: &RenderContext<Native>,
    ) -> Result<Statement> {
        Self::type_name_from_id(id, context).and_then(|_| {
            Ok(Statement::expression(Expression::call(
                value,
                Identifier::parse("writeTo")?,
                [Expression::identifier(writer)]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )))
        })
    }

    pub fn size_expression(
        id: EnumId,
        value: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        Self::type_name_from_id(id, context).and_then(|_| {
            Ok(Expression::call(
                value,
                Identifier::parse("wireSize")?,
                ArgumentList::default(),
            ))
        })
    }

    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: Option<&JniBridgeContract>,
        context: &RenderContext<Native>,
        error: bool,
    ) -> Result<Self> {
        let primitive = enumeration.repr().primitive();
        let name = Name::new(enumeration.name()).type_name();
        let receiver = Receiver {
            argument: KotlinPrimitive::new(primitive).native_argument(Expression::property(
                Expression::this(),
                Identifier::parse("value")?,
            ))?,
            writeback: None,
        };
        Ok(Self {
            name,
            error,
            body: Body::CStyle {
                value_type: KotlinPrimitive::new(primitive).native_type()?,
                repr: primitive,
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| CStyleVariant::from_c_style(variant, enumeration, error))
                    .collect::<Result<Vec<_>>>()?,
            },
            initializers: Self::initializer_calls(
                enumeration.initializers(),
                bridge,
                context,
                None,
            )?,
            static_methods: Self::methods(enumeration.methods(), None, bridge, context, None)?,
            instance_methods: Self::methods(
                enumeration.methods(),
                Some(receiver),
                bridge,
                context,
                None,
            )?,
        })
    }

    fn from_data(
        enumeration: &DataEnumDecl<Native>,
        bridge: Option<&JniBridgeContract>,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
        error: bool,
    ) -> Result<Self> {
        let name = Name::new(enumeration.name()).type_name();
        let receiver = Receiver {
            argument: Self::encode_expression(Expression::this())?,
            writeback: Some(name.clone()),
        };
        Ok(Self {
            name,
            error,
            body: Body::Data {
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| DataVariant::from_declaration(variant, context, package))
                    .collect::<Result<Vec<_>>>()?,
            },
            initializers: Self::initializer_calls(
                enumeration.initializers(),
                bridge,
                context,
                package,
            )?,
            static_methods: Self::methods(enumeration.methods(), None, bridge, context, package)?,
            instance_methods: Self::methods(
                enumeration.methods(),
                Some(receiver),
                bridge,
                context,
                package,
            )?,
        })
    }

    fn encode_expression(value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            value,
            Identifier::parse("toByteArray")?,
            Default::default(),
        ))
    }

    fn initializer_calls(
        initializers: &[InitializerDecl<Native>],
        bridge: Option<&JniBridgeContract>,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Vec<ExportedCall>> {
        bridge.map_or_else(
            || Ok(Vec::new()),
            |bridge| {
                initializers
                    .iter()
                    .map(|initializer| match package {
                        Some(package) => ExportedCall::new_with_record_package(
                            Name::new(initializer.name()).function()?,
                            initializer.symbol(),
                            initializer.callable(),
                            Vec::new(),
                            package,
                            bridge,
                            context,
                        ),
                        None => ExportedCall::new(
                            Name::new(initializer.name()).function()?,
                            initializer.symbol(),
                            initializer.callable(),
                            Vec::new(),
                            bridge,
                            context,
                        ),
                    })
                    .collect()
            },
        )
    }

    fn methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Option<Receiver>,
        bridge: Option<&JniBridgeContract>,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Vec<ExportedCall>> {
        bridge.map_or_else(
            || Ok(Vec::new()),
            |bridge| {
                methods
                    .iter()
                    .filter(|method| method.callable().receiver().is_some() == receiver.is_some())
                    .map(
                        |method| match (method.callable().receiver(), receiver.clone()) {
                            (Some(Receive::ByMutRef), Some(receiver)) => receiver
                                .writeback
                                .ok_or(Error::UnsupportedTarget {
                                    target: KotlinHost::TARGET,
                                    shape: "mutable c-style enum receiver",
                                })
                                .and_then(|writeback| {
                                    let mutation = match package {
                                        Some(package) => EncodedReceiverMutation::new(writeback)
                                            .with_record_package(package),
                                        None => EncodedReceiverMutation::new(writeback),
                                    };
                                    ExportedCall::new_encoded_receiver_mutation(
                                        Name::new(method.name()).function()?,
                                        method.target(),
                                        method.callable(),
                                        vec![receiver.argument],
                                        mutation,
                                        bridge,
                                        context,
                                    )
                                }),
                            (Some(Receive::ByRef | Receive::ByValue), Some(receiver)) => {
                                match package {
                                    Some(package) => ExportedCall::new_with_record_package(
                                        Name::new(method.name()).function()?,
                                        method.target(),
                                        method.callable(),
                                        vec![receiver.argument],
                                        package,
                                        bridge,
                                        context,
                                    ),
                                    None => ExportedCall::new(
                                        Name::new(method.name()).function()?,
                                        method.target(),
                                        method.callable(),
                                        vec![receiver.argument],
                                        bridge,
                                        context,
                                    ),
                                }
                            }
                            (None, None) => match package {
                                Some(package) => ExportedCall::new_with_record_package(
                                    Name::new(method.name()).function()?,
                                    method.target(),
                                    method.callable(),
                                    Vec::new(),
                                    package,
                                    bridge,
                                    context,
                                ),
                                None => ExportedCall::new(
                                    Name::new(method.name()).function()?,
                                    method.target(),
                                    method.callable(),
                                    Vec::new(),
                                    bridge,
                                    context,
                                ),
                            },
                            _ => Err(Error::UnsupportedTarget {
                                target: KotlinHost::TARGET,
                                shape: "enum method receiver",
                            }),
                        },
                    )
                    .collect()
            },
        )
    }
}

impl std::fmt::Display for Enumeration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.name, formatter)
    }
}

impl CStyleVariant {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }

    fn from_c_style(
        variant: &CStyleVariantDecl,
        enumeration: &CStyleEnumDecl<Native>,
        error: bool,
    ) -> Result<Self> {
        Ok(Self {
            name: match error {
                true => Name::new(variant.name()).variant()?,
                false => Name::new(variant.name()).enum_entry()?,
            },
            value: KotlinPrimitive::new(enumeration.repr().primitive())
                .native_integer_literal(variant.discriminant())?,
        })
    }
}

impl DataVariant {
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn tag(&self) -> &Expression {
        &self.tag
    }

    pub fn fields(&self) -> &[EncodedField] {
        &self.fields
    }

    pub fn read(&self) -> &Expression {
        &self.read
    }

    pub fn size(&self) -> &Expression {
        &self.size
    }

    pub fn tag_write(&self) -> &Statement {
        &self.tag_write
    }

    pub fn unit(&self) -> bool {
        self.fields.is_empty()
    }

    fn from_declaration(
        variant: &DataVariantDecl,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        let name = Name::new(variant.name()).variant()?;
        let tag = Self::tag_expression(variant.tag())?;
        let fields = Self::payload_fields(variant.payload(), context, package)?;
        let read = Self::read_expression(name.clone(), &fields);
        let size = fields
            .iter()
            .map(|field| field.size().clone())
            .fold(Expression::integer(4), Expression::add);
        let tag_write = Statement::expression(Expression::call(
            Expression::identifier(Identifier::parse("writer")?),
            Identifier::parse("writeU32")?,
            [tag.clone()].into_iter().collect::<ArgumentList>(),
        ));
        Ok(Self {
            name,
            tag,
            fields,
            read,
            size,
            tag_write,
        })
    }

    fn payload_fields(
        payload: &DataVariantPayload,
        context: &RenderContext<Native>,
        package: Option<&KotlinPackage>,
    ) -> Result<Vec<EncodedField>> {
        let reader = Identifier::parse("reader")?;
        let writer = Identifier::parse("writer")?;
        let current = Expression::this();
        match payload {
            DataVariantPayload::Unit => Ok(Vec::new()),
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => fields
                .iter()
                .map(|field| match package {
                    Some(package) => EncodedField::from_enum_payload(
                        field,
                        context,
                        &reader,
                        &writer,
                        current.clone(),
                        package,
                    ),
                    None => EncodedField::from_declaration(
                        field,
                        context,
                        &reader,
                        &writer,
                        current.clone(),
                    ),
                })
                .collect(),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown data enum payload",
            }),
        }
    }

    fn read_expression(name: Identifier, fields: &[EncodedField]) -> Expression {
        match fields.is_empty() {
            true => Expression::identifier(name),
            false => Expression::construct(
                TypeName::new(name.to_string()),
                fields
                    .iter()
                    .map(|field| field.read().clone())
                    .collect::<ArgumentList>(),
            ),
        }
    }

    fn tag_expression(tag: VariantTag) -> Result<Expression> {
        Ok(Expression::integer(tag.get()).convert(Identifier::parse("toUInt")?))
    }
}

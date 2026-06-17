use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CStyleEnumDecl, DataEnumDecl, DeclarationRef, EnumDecl, EnumId, ExportedMethodDecl,
    InitializerDecl, Native, NativeSymbol, Receive,
};

use crate::{
    bridge::{
        c::{self, identifier::Identifier, syntax::TypeSyntax},
        python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{argument, direct_vector, function, primitive, result},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/enumeration.c", escape = "none")]
struct CStyleTemplate {
    class_name: String,
    c_type: String,
    registration: String,
    members_by_wire_tag: String,
    member_names: String,
    member_native_values: String,
    register_method: String,
    register_wrapper: String,
    load_member: String,
    parser: String,
    boxer: String,
    box_from_wire_tag: String,
    native_to_wire_tag: String,
    repr_parser: String,
    repr_boxer: String,
    variants: Vec<Variant>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/data_enum.c", escape = "none")]
struct DataTemplate {
    class_name: String,
    type_object: String,
    register_method: String,
    register_wrapper: String,
    wire_encoder: String,
    owned_decoder: String,
}

pub struct Wrapper {
    symbols: Symbols,
    shape: Shape,
    method: ExtensionMethod,
    callables: Vec<function::Wrapper>,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, bridge, context),
            EnumDecl::Data(enumeration) => Self::from_data(enumeration, bridge, context),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown enum",
            }),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        let symbols = self.symbols;
        let source = match self.shape {
            Shape::CStyle {
                variants,
                primitive,
            } => {
                let c_type = symbols.c_type()?.to_owned();
                let registration = symbols.registration()?;
                let members_by_wire_tag = symbols.members_by_wire_tag()?;
                let member_names = symbols.member_names()?;
                let member_native_values = symbols.member_native_values()?;
                let load_member = symbols.load_member()?;
                let box_from_wire_tag = symbols.box_from_wire_tag()?;
                let native_to_wire_tag = symbols.native_to_wire_tag()?;
                CStyleTemplate {
                    class_name: symbols.class_name,
                    c_type,
                    registration,
                    members_by_wire_tag,
                    member_names,
                    member_native_values,
                    register_method: symbols.register_method,
                    register_wrapper: symbols.register_wrapper,
                    load_member,
                    parser: symbols.parser,
                    boxer: symbols.boxer,
                    box_from_wire_tag,
                    native_to_wire_tag,
                    repr_parser: primitive.parser()?.to_owned(),
                    repr_boxer: primitive.boxer()?.to_owned(),
                    variants,
                }
                .render()?
            }
            Shape::Data => {
                let type_object = symbols.type_object()?;
                DataTemplate {
                    class_name: symbols.class_name,
                    type_object,
                    register_method: symbols.register_method,
                    register_wrapper: symbols.register_wrapper,
                    wire_encoder: symbols.parser,
                    owned_decoder: symbols.boxer,
                }
                .render()?
            }
        };
        let callables = self
            .callables
            .into_iter()
            .map(function::Wrapper::render)
            .collect::<Result<Vec<_>>>()?;
        Ok(Emitted::primary(
            std::iter::once(source)
                .chain(
                    callables
                        .into_iter()
                        .map(|emitted| emitted.primary_chunk().as_str().to_owned()),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        std::iter::once(&self.method).chain(self.callables.iter().map(function::Wrapper::method))
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        match self.shape {
            Shape::CStyle { primitive, .. } => Some(primitive),
            Shape::Data => None,
        }
    }

    pub fn cleanup(&self) -> String {
        match &self.shape {
            Shape::CStyle { .. } => format!(
                "boltffi_python_clear_c_style_enum_registration(&{})",
                self.symbols.registration.as_deref().unwrap_or("")
            ),
            Shape::Data => format!(
                "Py_CLEAR({})",
                self.symbols.type_object.as_deref().unwrap_or("")
            ),
        }
    }

    pub fn needs_owned_buffer(&self) -> bool {
        matches!(self.shape, Shape::Data)
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.callables
            .iter()
            .filter_map(function::Wrapper::owned_buffer)
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.callables
            .iter()
            .flat_map(function::Wrapper::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.callables
            .iter()
            .flat_map(function::Wrapper::direct_vector_elements)
    }

    pub fn has_string_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_raw_wire_argument)
    }

    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if enumeration.variants().is_empty() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "empty c-style enum",
            });
        }
        let c_enum =
            bridge
                .source_c_style_enum(enumeration.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "c-style enum without C typedef",
                })?;
        if enumeration.variants().len() != c_enum.variants().len() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "enum variant mismatch",
            });
        }
        let symbols = Symbols::from_c_style(enumeration, c_enum)?;
        let variants = enumeration
            .variants()
            .iter()
            .zip(c_enum.variants())
            .enumerate()
            .map(|(index, (variant, c_variant))| Variant::new(index, variant, c_variant))
            .collect::<Result<Vec<_>>>()?;
        let method = ExtensionMethod::new(
            symbols.register_method.clone(),
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::c_style_callables(enumeration, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::CStyle {
                variants,
                primitive: primitive::Runtime::new(enumeration.repr().primitive()),
            },
            method,
            callables,
        })
    }

    fn from_data(
        enumeration: &DataEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::from_data(enumeration)?;
        let method = ExtensionMethod::new(
            symbols.register_method.clone(),
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::data_callables(enumeration, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::Data,
            method,
            callables,
        })
    }

    fn c_style_callables(
        enumeration: &CStyleEnumDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Wrapper>> {
        let initializers = enumeration
            .initializers()
            .iter()
            .filter(|initializer| function::Wrapper::supports(initializer.callable()))
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = enumeration
            .methods()
            .iter()
            .filter(|method| {
                function::Wrapper::supports(method.callable())
                    && !matches!(method.callable().receiver(), Some(Receive::ByMutRef))
            })
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|_| {
                        argument::Conversion::c_style_enum_receiver(
                            enumeration.id(),
                            bridge,
                            context,
                        )
                    })
                    .transpose()?
                    .into_iter()
                    .collect();
                Self::method(method, symbols, receiver, bridge, context)
            });
        initializers.chain(methods).collect()
    }

    fn data_callables(
        enumeration: &DataEnumDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Wrapper>> {
        let initializers = enumeration
            .initializers()
            .iter()
            .filter(|initializer| function::Wrapper::supports(initializer.callable()))
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = enumeration
            .methods()
            .iter()
            .filter(|method| {
                function::Wrapper::supports(method.callable())
                    && !matches!(method.callable().receiver(), Some(Receive::ByMutRef))
            })
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|receive| {
                        argument::Conversion::data_enum_receiver(
                            enumeration.id(),
                            receive,
                            bridge,
                            context,
                        )
                    })
                    .transpose()?
                    .into_iter()
                    .collect();
                Self::method(method, symbols, receiver, bridge, context)
            });
        initializers.chain(methods).collect()
    }

    fn initializer(
        initializer: &InitializerDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<function::Wrapper> {
        function::Wrapper::from_export(
            symbols.initializer(initializer.name()),
            initializer.symbol(),
            initializer.callable(),
            Vec::new(),
            bridge,
            context,
        )
    }

    fn method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        symbols: &Symbols,
        receiver: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<function::Wrapper> {
        function::Wrapper::from_export(
            symbols.method(method.name()),
            method.target(),
            method.callable(),
            receiver,
            bridge,
            context,
        )
    }
}

pub struct Symbols {
    class_name: String,
    stem: String,
    c_type: Option<String>,
    type_object: Option<String>,
    registration: Option<String>,
    members_by_wire_tag: Option<String>,
    member_names: Option<String>,
    member_native_values: Option<String>,
    register_method: String,
    register_wrapper: String,
    load_member: Option<String>,
    parser: String,
    boxer: String,
    direct_vec_parser: Option<String>,
    direct_vec_decoder: Option<String>,
    box_from_wire_tag: Option<String>,
    native_to_wire_tag: Option<String>,
}

impl Symbols {
    pub fn from_enum_id(
        enum_id: EnumId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let enumeration = context
            .bindings()
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration))
                    if enumeration.id() == enum_id =>
                {
                    Some(EnumDecl::CStyle(enumeration.clone()))
                }
                DeclarationRef::Enum(EnumDecl::Data(enumeration))
                    if enumeration.id() == enum_id =>
                {
                    Some(EnumDecl::Data(enumeration.clone()))
                }
                DeclarationRef::Enum(_)
                | DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum id without declaration",
            })?;
        match enumeration {
            EnumDecl::CStyle(enumeration) => {
                let c_enum =
                    bridge
                        .source_c_style_enum(enum_id)
                        .ok_or(Error::UnsupportedTarget {
                            target: "python",
                            shape: "c-style enum without C typedef",
                        })?;
                Self::from_c_style(&enumeration, c_enum)
            }
            EnumDecl::Data(enumeration) => Self::from_data(&enumeration),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown enum declaration",
            }),
        }
    }

    pub fn c_type(&self) -> Result<&str> {
        self.c_type.as_deref().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C type",
        })
    }

    pub fn parser(&self) -> &str {
        &self.parser
    }

    pub fn boxer(&self) -> &str {
        &self.boxer
    }

    pub fn direct_vec_parser(&self) -> Result<&str> {
        self.direct_vec_parser
            .as_deref()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no direct vector parser",
            })
    }

    pub fn direct_vec_decoder(&self) -> Result<&str> {
        self.direct_vec_decoder
            .as_deref()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no direct vector decoder",
            })
    }

    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    pub fn register_method(&self) -> &str {
        &self.register_method
    }

    pub fn initializer(&self, name: &boltffi_binding::CanonicalName) -> String {
        self.callable(name)
    }

    pub fn method(&self, name: &boltffi_binding::CanonicalName) -> String {
        self.callable(name)
    }

    pub fn from_c_style(enumeration: &CStyleEnumDecl<Native>, c_enum: &c::Enum) -> Result<Self> {
        let stem = Identifier::escape(Name::new(enumeration.name()).function())?.to_string();
        Ok(Self {
            class_name: Name::new(enumeration.name()).class(),
            stem: stem.clone(),
            c_type: Some(TypeSyntax::new(&c::Type::Named(c_enum.name().to_owned())).anonymous()?),
            type_object: None,
            registration: Some(format!("boltffi_python_{stem}_registration")),
            members_by_wire_tag: Some(format!("boltffi_python_{stem}_members_by_wire_tag")),
            member_names: Some(format!("boltffi_python_{stem}_member_names")),
            member_native_values: Some(format!("boltffi_python_{stem}_member_native_values")),
            register_method: format!("_register_{stem}"),
            register_wrapper: format!("boltffi_python_wrapper_register_{stem}"),
            load_member: Some(format!("boltffi_python_load_{stem}_member")),
            parser: format!("boltffi_python_parse_{stem}"),
            boxer: format!("boltffi_python_box_{stem}"),
            direct_vec_parser: Some(format!("boltffi_python_parse_vec_{stem}")),
            direct_vec_decoder: Some(format!("boltffi_python_decode_owned_vec_{stem}")),
            box_from_wire_tag: Some(format!("boltffi_python_box_{stem}_from_wire_tag")),
            native_to_wire_tag: Some(format!("boltffi_python_{stem}_native_to_wire_tag")),
        })
    }

    pub fn from_data(enumeration: &DataEnumDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(enumeration.name()).function())?.to_string();
        Ok(Self {
            class_name: Name::new(enumeration.name()).class(),
            stem: stem.clone(),
            c_type: None,
            type_object: Some(format!("boltffi_python_{stem}_type")),
            registration: None,
            members_by_wire_tag: None,
            member_names: None,
            member_native_values: None,
            register_method: format!("_register_{stem}"),
            register_wrapper: format!("boltffi_python_wrapper_register_{stem}"),
            load_member: None,
            parser: format!("boltffi_python_wire_{stem}"),
            boxer: format!("boltffi_python_decode_owned_{stem}"),
            direct_vec_parser: None,
            direct_vec_decoder: None,
            box_from_wire_tag: None,
            native_to_wire_tag: None,
        })
    }

    fn registration(&self) -> Result<String> {
        self.registration.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C-style registration",
        })
    }

    fn members_by_wire_tag(&self) -> Result<String> {
        self.members_by_wire_tag
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style member table",
            })
    }

    fn member_names(&self) -> Result<String> {
        self.member_names.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C-style member names",
        })
    }

    fn member_native_values(&self) -> Result<String> {
        self.member_native_values
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style native values",
            })
    }

    fn load_member(&self) -> Result<String> {
        self.load_member.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "data enum has no C-style member loader",
        })
    }

    fn box_from_wire_tag(&self) -> Result<String> {
        self.box_from_wire_tag
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style tag boxer",
            })
    }

    fn native_to_wire_tag(&self) -> Result<String> {
        self.native_to_wire_tag
            .clone()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum has no C-style native tag mapper",
            })
    }

    fn type_object(&self) -> Result<String> {
        self.type_object.clone().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "c-style enum has no Python type object",
        })
    }

    fn callable(&self, name: &boltffi_binding::CanonicalName) -> String {
        format!("_boltffi_{}_{}", self.stem, Name::new(name).function())
    }
}

enum Shape {
    CStyle {
        variants: Vec<Variant>,
        primitive: primitive::Runtime,
    },
    Data,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonClass {
    class_name: String,
    register_method: String,
    variants: Vec<PythonVariant>,
}

impl PythonClass {
    pub fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        let c_enum =
            bridge
                .source_c_style_enum(enumeration.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "c-style enum package without C typedef",
                })?;
        let symbols = Symbols::from_c_style(enumeration, c_enum)?;
        Ok(Self {
            class_name: symbols.class_name().to_owned(),
            register_method: symbols.register_method().to_owned(),
            variants: enumeration
                .variants()
                .iter()
                .map(PythonVariant::from_variant)
                .collect(),
        })
    }

    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    pub fn register_method(&self) -> &str {
        &self.register_method
    }

    pub fn variants(&self) -> &[PythonVariant] {
        &self.variants
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PythonVariant {
    name: String,
    value: i128,
}

impl PythonVariant {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn value(&self) -> i128 {
        self.value
    }

    fn from_variant(variant: &boltffi_binding::CStyleVariantDecl) -> Self {
        Self {
            name: Name::new(variant.name()).enum_member(),
            value: variant.discriminant().get(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Variant {
    member_name: String,
    native_value: String,
    wire_tag: usize,
    member_index: usize,
}

impl Variant {
    fn new(
        index: usize,
        variant: &boltffi_binding::CStyleVariantDecl,
        c_variant: &c::EnumVariant,
    ) -> Result<Self> {
        Ok(Self {
            member_name: Name::new(variant.name()).enum_member(),
            native_value: Identifier::parse(c_variant.name())?.to_string(),
            wire_tag: index,
            member_index: index,
        })
    }
}

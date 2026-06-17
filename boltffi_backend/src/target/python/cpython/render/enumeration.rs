use askama::Template as AskamaTemplate;
use boltffi_binding::{CStyleEnumDecl, DeclarationRef, EnumDecl, EnumId, Native};

use crate::{
    bridge::{
        c::{self, identifier::Identifier, syntax::TypeSyntax},
        python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{cpython::render::primitive, name_style::Name},
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/enumeration.c", escape = "none")]
struct Template {
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

pub struct Wrapper {
    symbols: Symbols,
    variants: Vec<Variant>,
    method: ExtensionMethod,
    primitive: primitive::Runtime,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &EnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        match declaration {
            EnumDecl::CStyle(enumeration) => Self::from_c_style(enumeration, bridge),
            EnumDecl::Data(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown enum",
            }),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        let source = Template {
            class_name: self.symbols.class_name,
            c_type: self.symbols.c_type,
            registration: self.symbols.registration,
            members_by_wire_tag: self.symbols.members_by_wire_tag,
            member_names: self.symbols.member_names,
            member_native_values: self.symbols.member_native_values,
            register_method: self.symbols.register_method,
            register_wrapper: self.symbols.register_wrapper,
            load_member: self.symbols.load_member,
            parser: self.symbols.parser,
            boxer: self.symbols.boxer,
            box_from_wire_tag: self.symbols.box_from_wire_tag,
            native_to_wire_tag: self.symbols.native_to_wire_tag,
            repr_parser: self.primitive.parser()?.to_owned(),
            repr_boxer: self.primitive.boxer()?.to_owned(),
            variants: self.variants,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    pub fn method(&self) -> &ExtensionMethod {
        &self.method
    }

    pub fn primitive(&self) -> primitive::Runtime {
        self.primitive
    }

    pub fn cleanup(&self) -> String {
        format!(
            "boltffi_python_clear_c_style_enum_registration(&{})",
            self.symbols.registration
        )
    }

    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
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
        Ok(Self {
            symbols,
            variants,
            method,
            primitive: primitive::Runtime::new(enumeration.repr().primitive()),
        })
    }
}

pub struct Symbols {
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
                    Some(enumeration)
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
                shape: "enum id without c-style declaration",
            })?;
        let c_enum = bridge
            .source_c_style_enum(enum_id)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "c-style enum without C typedef",
            })?;
        Self::from_c_style(enumeration, c_enum)
    }

    pub fn c_type(&self) -> &str {
        &self.c_type
    }

    pub fn parser(&self) -> &str {
        &self.parser
    }

    pub fn boxer(&self) -> &str {
        &self.boxer
    }

    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    pub fn register_method(&self) -> &str {
        &self.register_method
    }

    pub fn from_c_style(enumeration: &CStyleEnumDecl<Native>, c_enum: &c::Enum) -> Result<Self> {
        let stem = Identifier::escape(Name::new(enumeration.name()).function())?.to_string();
        Ok(Self {
            class_name: Name::new(enumeration.name()).class(),
            c_type: TypeSyntax::new(&c::Type::Named(c_enum.name().to_owned())).anonymous()?,
            registration: format!("boltffi_python_{stem}_registration"),
            members_by_wire_tag: format!("boltffi_python_{stem}_members_by_wire_tag"),
            member_names: format!("boltffi_python_{stem}_member_names"),
            member_native_values: format!("boltffi_python_{stem}_member_native_values"),
            register_method: format!("_register_{stem}"),
            register_wrapper: format!("boltffi_python_wrapper_register_{stem}"),
            load_member: format!("boltffi_python_load_{stem}_member"),
            parser: format!("boltffi_python_parse_{stem}"),
            boxer: format!("boltffi_python_box_{stem}"),
            box_from_wire_tag: format!("boltffi_python_box_{stem}_from_wire_tag"),
            native_to_wire_tag: format!("boltffi_python_{stem}_native_to_wire_tag"),
        })
    }
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

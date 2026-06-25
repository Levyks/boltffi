use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ConstantDecl, ConstantValueDecl, DefaultValue, ExportedCallable, FloatValue, Native,
    NativeSymbol, Primitive, TypeRef,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{function::ExportedCall, type_name::KotlinType},
        syntax::{Expression, Identifier, Literal, TypeName},
    },
};

const KOTLIN_TARGET: &str = "kotlin";

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/constant.kt", escape = "none")]
struct ConstantTemplate {
    constant: Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
    inline: Option<Inline>,
    accessor: Option<ExportedCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Inline {
    name: Identifier,
    ty: TypeName,
    value: Expression,
}

impl Constant {
    pub fn from_declaration(
        declaration: &ConstantDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration.value() {
            ConstantValueDecl::Inline { ty, value, .. } => Ok(Self {
                inline: Some(Inline::new(declaration, ty, value, context)?),
                accessor: None,
            }),
            ConstantValueDecl::Accessor { symbol, callable } => Ok(Self {
                inline: None,
                accessor: Some(Self::build_accessor(
                    declaration,
                    symbol,
                    callable,
                    bridge,
                    context,
                )?),
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown constant value",
            }),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            ConstantTemplate { constant: self }
                .render()?
                .trim()
                .to_owned(),
        ))
    }

    pub fn inline(&self) -> Option<&Inline> {
        self.inline.as_ref()
    }

    pub fn accessor(&self) -> Option<&ExportedCall> {
        self.accessor.as_ref()
    }

    fn build_accessor(
        declaration: &ConstantDecl<Native>,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<ExportedCall> {
        let call = ExportedCall::new(
            Name::new(declaration.name()).function()?,
            symbol,
            callable,
            Vec::new(),
            bridge,
            context,
        )?;
        if call.async_call().is_some() {
            return Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "async constant accessor",
            });
        }
        if call.returns().is_none() {
            return Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "constant accessor without return",
            });
        }
        Ok(call)
    }
}

impl Inline {
    fn new(
        declaration: &ConstantDecl<Native>,
        ty: &TypeRef,
        value: &DefaultValue,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            name: Name::new(declaration.name()).function()?,
            ty: KotlinType::type_ref(ty, context)?,
            value: Self::render_value(ty, value)?,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    pub fn value(&self) -> &Expression {
        &self.value
    }

    fn render_value(ty: &TypeRef, value: &DefaultValue) -> Result<Expression> {
        match value {
            DefaultValue::Bool(value) => Ok(Expression::bool(*value)),
            DefaultValue::Integer(value) => match ty {
                TypeRef::Primitive(primitive) => {
                    KotlinPrimitive::new(*primitive).integer_literal(*value)
                }
                _ => Err(Error::UnsupportedTarget {
                    target: KOTLIN_TARGET,
                    shape: "integer constant type",
                }),
            },
            DefaultValue::Float(value) => Self::float(*value, ty),
            DefaultValue::String(value) => Ok(Expression::literal(Literal::string(value))),
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } => Ok(Expression::property(
                Name::new(enum_name).type_name(),
                Name::new(variant_name).variant()?,
            )),
            DefaultValue::Null => Ok(Expression::null()),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown constant literal",
            }),
        }
    }

    fn float(value: FloatValue, ty: &TypeRef) -> Result<Expression> {
        match ty {
            TypeRef::Primitive(Primitive::F32) => Ok(Expression::float(value.to_f64(), true)),
            TypeRef::Primitive(Primitive::F64) => Ok(Expression::float(value.to_f64(), false)),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "float constant type",
            }),
        }
    }
}

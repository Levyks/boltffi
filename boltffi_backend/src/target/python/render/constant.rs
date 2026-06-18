use boltffi_binding::{ConstantDecl, ConstantValueDecl, DefaultValue, Native, TypeRef};

use crate::{
    core::{Error, Result},
    target::python::name_style::Name,
};

use super::{Package, callable::ReturnStub, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstantStub {
    pub python_name: String,
    pub annotation: String,
    pub expression: String,
    uses_wire_helpers: bool,
}

impl ConstantStub {
    pub fn from_declaration(
        constant: &ConstantDecl<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match constant.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                Self::from_inline(constant, ty, value, package)
            }
            ConstantValueDecl::Accessor { callable, .. } => {
                let returned = ReturnStub::from_plan(callable.returns().plan(), package)?;
                let native_call = format!("_native.{}()", Name::new(constant.name()).function());
                Ok(Self {
                    python_name: Name::new(constant.name()).function(),
                    annotation: returned.annotation().to_owned(),
                    expression: returned.expression(native_call),
                    uses_wire_helpers: returned.uses_wire_helpers(),
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown constant value package",
            }),
        }
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn top_level_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("constant `{}`", self.python_name),
        )
    }

    fn from_inline(
        constant: &ConstantDecl<Native>,
        ty: &TypeRef,
        value: &DefaultValue,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        Ok(Self {
            python_name: Name::new(constant.name()).function(),
            annotation: TypeHint::from_type_ref(ty, package)?.into_string(),
            expression: ConstantExpression::new(value, package)?.into_string(),
            uses_wire_helpers: false,
        })
    }
}

struct ConstantExpression {
    expression: String,
}

impl ConstantExpression {
    fn new(value: &DefaultValue, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            expression: match value {
                DefaultValue::Bool(value) => Self::bool(*value),
                DefaultValue::Integer(value) => value.get().to_string(),
                DefaultValue::Float(value) => Self::float(value.to_f64()),
                DefaultValue::String(value) => Package::literal(value),
                DefaultValue::EnumVariant {
                    enum_name,
                    variant_name,
                } => package.enum_variant_expression(enum_name, variant_name)?,
                DefaultValue::Null => "None".to_owned(),
                _ => {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "unknown constant literal",
                    });
                }
            },
        })
    }

    fn into_string(self) -> String {
        self.expression
    }

    fn bool(value: bool) -> String {
        match value {
            true => "True".to_owned(),
            false => "False".to_owned(),
        }
    }

    fn float(value: f64) -> String {
        if value.is_nan() {
            return "float(\"nan\")".to_owned();
        }
        if value == f64::INFINITY {
            return "float(\"inf\")".to_owned();
        }
        if value == f64::NEG_INFINITY {
            return "float(\"-inf\")".to_owned();
        }
        if value == 0.0 && value.is_sign_negative() {
            return "-0.0".to_owned();
        }
        value.to_string()
    }
}

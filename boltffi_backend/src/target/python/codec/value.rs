use boltffi_binding::{BinderId, FieldKey, ValueRef, ValueRoot};

use crate::{
    core::{Error, Result},
    target::python::{
        name_style::Name,
        syntax::{Expression, Identifier, Literal},
    },
};

pub struct ValueExpression<'value> {
    value: &'value ValueRef,
}

impl<'value> ValueExpression<'value> {
    pub fn new(value: &'value ValueRef) -> Self {
        Self { value }
    }

    pub fn root(value: &ValueRef) -> Result<Expression> {
        match value.root() {
            ValueRoot::SelfValue => Identifier::parse("self").map(Expression::identifier),
            ValueRoot::Named(name) | ValueRoot::Local(name) => {
                Name::new(name).function().map(Expression::identifier)
            }
            ValueRoot::Binder(binder) => Ok(Expression::identifier(Self::binder(*binder)?)),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown codec value root",
            }),
        }
    }

    pub fn binder(binder: BinderId) -> Result<Identifier> {
        Identifier::parse(format!("__boltffi_value_{}", binder.raw()))
    }

    pub fn render(self) -> Result<Expression> {
        let root = Self::root(self.value)?;
        self.value.path().iter().try_fold(root, Self::field)
    }

    pub fn field(expression: Expression, field: &FieldKey) -> Result<Expression> {
        Ok(match field {
            FieldKey::Named(name) => Expression::attribute(expression, Name::new(name).function()?),
            FieldKey::Position(position) => Expression::subscript(
                expression,
                Expression::literal(Literal::integer(i128::from(*position))),
            ),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown codec value field",
                });
            }
        })
    }
}

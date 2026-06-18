use boltffi_binding::{BinderId, FieldKey, ValueRef, ValueRoot};

use crate::{
    core::{Error, Result},
    target::python::name_style::Name,
};

pub struct ValueExpression<'value> {
    value: &'value ValueRef,
}

impl<'value> ValueExpression<'value> {
    pub fn new(value: &'value ValueRef) -> Self {
        Self { value }
    }

    pub fn root(value: &ValueRef) -> Result<String> {
        match value.root() {
            ValueRoot::SelfValue => Ok("self".to_owned()),
            ValueRoot::Named(name) | ValueRoot::Local(name) => Ok(Name::new(name).function()),
            ValueRoot::Binder(binder) => Ok(Self::binder(*binder)),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown codec value root",
            }),
        }
    }

    pub fn binder(binder: BinderId) -> String {
        format!("__boltffi_value_{}", binder.raw())
    }

    pub fn render(self) -> Result<String> {
        let root = Self::root(self.value)?;
        self.value.path().iter().try_fold(root, Self::field)
    }

    pub fn field(expression: String, field: &FieldKey) -> Result<String> {
        Ok(match field {
            FieldKey::Named(name) => format!("{expression}.{}", Name::new(name).function()),
            FieldKey::Position(position) => format!("{expression}[{position}]"),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown codec value field",
                });
            }
        })
    }
}

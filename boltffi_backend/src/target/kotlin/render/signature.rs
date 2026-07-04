use crate::target::kotlin::syntax::{Identifier, TypeName};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeName,
}

impl Parameter {
    pub fn new(name: Identifier, ty: TypeName) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }
}

use boltffi_binding::{ReadPlan, WritePlan};

use crate::{
    bridge::c::{ArgumentList, Expression, Identifier},
    core::Result,
    target::python::{
        codec::AdapterKey, cpython::render::direct_vector, cpython::render::primitive,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowedPayload {
    expression: Expression,
}

impl BorrowedPayload {
    pub fn read(plan: &ReadPlan, pointer: Identifier, length: Identifier) -> Result<Self> {
        Ok(Self {
            expression: Expression::call(
                AdapterKey::read(plan).c_decoder()?,
                ArgumentList::from_iter([
                    Expression::identifier(pointer),
                    Expression::identifier(length),
                ]),
            ),
        })
    }

    pub fn expression(self) -> Expression {
        self.expression
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        None
    }

    pub fn has_string(&self) -> bool {
        false
    }

    pub fn has_bytes(&self) -> bool {
        false
    }

    pub fn has_raw_wire(&self) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedPayload {
    parser: Identifier,
}

impl OwnedPayload {
    pub fn write(plan: &WritePlan) -> Result<Self> {
        Ok(Self {
            parser: AdapterKey::write(plan).c_encoder()?,
        })
    }

    pub fn parser(&self) -> &Identifier {
        &self.parser
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        None
    }

    pub fn direct_vector(&self) -> Option<direct_vector::Element> {
        None
    }

    pub fn has_string(&self) -> bool {
        false
    }

    pub fn has_bytes(&self) -> bool {
        false
    }

    pub fn has_raw_wire(&self) -> bool {
        false
    }
}

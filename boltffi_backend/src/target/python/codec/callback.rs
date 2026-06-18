use boltffi_binding::{ReadPlan, WritePlan};

use crate::target::python::{
    codec::AdapterKey, cpython::render::direct_vector, cpython::render::primitive,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BorrowedPayload {
    expression: String,
}

impl BorrowedPayload {
    pub fn read(plan: &ReadPlan, pointer: &str, length: &str) -> Self {
        Self {
            expression: format!(
                "{}({pointer}, {length})",
                AdapterKey::read(plan).c_decoder()
            ),
        }
    }

    pub fn expression(self) -> String {
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
    parser: String,
}

impl OwnedPayload {
    pub fn write(plan: &WritePlan) -> Self {
        Self {
            parser: AdapterKey::write(plan).c_encoder(),
        }
    }

    pub fn parser(&self) -> &str {
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

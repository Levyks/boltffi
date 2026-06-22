//! Direct-record value layout for JNI.
//!
//! The C bridge already knows the concrete record ABI type. This module captures
//! the fixed byte length and C type spelling needed to move that record through
//! a Java byte array without reinterpreting the record fields.

use crate::bridge::c::{self, Identifier, TypeFragment};

/// Direct record value carried through JNI as a fixed-size byte array.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RecordValue {
    c_type: Identifier,
}

impl RecordValue {
    /// Returns the C typedef used by the C bridge.
    pub fn c_type(&self) -> &Identifier {
        &self.c_type
    }

    /// Returns the C typedef as a type fragment.
    pub fn c_type_fragment(&self) -> TypeFragment {
        TypeFragment::new(self.c_type.to_string())
    }

    /// Creates a direct-record JNI value from a C ABI type.
    pub fn from_c_type(ty: &c::Type) -> Option<Self> {
        match ty {
            c::Type::DirectRecord(c_type) => Some(Self {
                c_type: c_type.clone(),
            }),
            _ => None,
        }
    }
}

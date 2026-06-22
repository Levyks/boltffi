//! Byte-array parameters for JNI native methods.
//!
//! Encoded parameters cross the JVM boundary as `jbyteArray`. The C bridge does
//! not accept Java objects, so this contract records the Java parameter and the
//! local pointer and length variables passed into the C ABI call.

use crate::{
    bridge::c::{self, Identifier},
    core::Result,
};

/// Byte-array JNI parameter mapped to pointer and length C bridge arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BytesParameter {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
}

impl BytesParameter {
    /// Returns the generated JNI byte-array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the local pointer variable passed to the C bridge.
    pub fn pointer(&self) -> &Identifier {
        &self.pointer
    }

    /// Returns the local length variable passed to the C bridge.
    pub fn length(&self) -> &Identifier {
        &self.length
    }

    /// Creates a byte-array parameter from a C byte-slice parameter group.
    pub fn from_c_group(bytes: &c::ByteSliceParameter) -> Result<Self> {
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", bytes.name()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", bytes.name()))?,
            name: Identifier::escape(bytes.name())?,
        })
    }
}

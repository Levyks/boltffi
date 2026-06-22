//! Encoded byte-array parameters passed from Java into Rust.
//!
//! Encoded values enter JNI as `jbyteArray`, but the lower C bridge accepts only
//! borrowed bytes: pointer plus length. That conversion has a lifetime rule. The
//! generated method must borrow the Java array, call the C bridge while the
//! borrow is alive, then release the array on every exit path.
//!
//! This module records the stable names for that borrow. It does not decide what
//! the bytes mean and it does not inspect codec plans. By this point lowering
//! has already selected an encoded parameter; the JNI contract only describes
//! how the Java array is held long enough to reach the C ABI.

use crate::{
    bridge::c::{self, Identifier},
    core::Result,
};

/// A Java byte-array parameter borrowed as C pointer and length arguments.
///
/// The parameter owns the Java-facing name and the generated local variables
/// used by the native method body. Cleanup is rendered from these same names so
/// the borrow and release paths stay tied together.
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

//! Setup views for closure arguments.
//!
//! Closure templates need grouped setup data for byte arrays, direct vectors, and
//! nested handles. This module exposes those views from the typed closure
//! argument contract.

use super::{
    ClosureArgument, ClosureArgumentKind, ClosureBytesArgument, ClosureDirectVectorArgument,
    ClosureHandleArgument,
};

impl ClosureArgument {
    /// Returns the byte-array argument when the JVM receives encoded bytes.
    pub fn call_bytes(&self) -> Option<&ClosureBytesArgument> {
        match &self.kind {
            ClosureArgumentKind::Scalar(_) => None,
            ClosureArgumentKind::Bytes(argument) => Some(argument),
            ClosureArgumentKind::DirectVector(_) => None,
            ClosureArgumentKind::Closure(_) => None,
        }
    }

    /// Returns the byte-array argument when the JVM sends encoded bytes.
    pub fn handle_bytes(&self) -> Option<&ClosureBytesArgument> {
        self.call_bytes()
    }

    /// Returns the direct-vector argument when the JVM receives an array.
    pub fn call_direct_vector(&self) -> Option<&ClosureDirectVectorArgument> {
        match &self.kind {
            ClosureArgumentKind::DirectVector(argument) => Some(argument),
            ClosureArgumentKind::Scalar(_)
            | ClosureArgumentKind::Bytes(_)
            | ClosureArgumentKind::Closure(_) => None,
        }
    }

    /// Returns the closure-handle argument when the JVM receives a nested closure.
    pub fn call_closure(&self) -> Option<&ClosureHandleArgument> {
        match &self.kind {
            ClosureArgumentKind::Closure(argument) => Some(argument),
            ClosureArgumentKind::Scalar(_)
            | ClosureArgumentKind::Bytes(_)
            | ClosureArgumentKind::DirectVector(_) => None,
        }
    }

    /// Returns the direct-vector argument when the JVM sends an array.
    pub fn handle_direct_vector(&self) -> Option<&ClosureDirectVectorArgument> {
        self.call_direct_vector()
    }
}

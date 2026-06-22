//! C-parameter projection for closure calls.
//!
//! Closure arguments can expand to one C parameter or a grouped set of C
//! parameters. This module exposes the flat C parameter list accepted by the
//! generated closure trampoline.

use super::{ClosureArgument, ClosureArgumentKind, ClosureCParameter};

impl ClosureArgument {
    /// Returns the C parameters accepted by the closure call trampoline.
    pub fn c_parameters(&self) -> Vec<ClosureCParameter> {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.c_parameters(),
            ClosureArgumentKind::Bytes(argument) => argument.c_parameters(),
            ClosureArgumentKind::DirectVector(argument) => argument.c_parameters(),
            ClosureArgumentKind::Closure(argument) => argument.c_parameters(),
        }
    }

    /// Returns the C parameters accepted by the Rust-owned closure handle entrypoint.
    pub fn handle_parameters(&self) -> Vec<ClosureCParameter> {
        match &self.kind {
            ClosureArgumentKind::Scalar(argument) => argument.handle_parameters(),
            ClosureArgumentKind::Bytes(argument) => argument.handle_parameters(),
            ClosureArgumentKind::DirectVector(argument) => argument.handle_parameters(),
            ClosureArgumentKind::Closure(argument) => argument.handle_parameters(),
        }
    }
}

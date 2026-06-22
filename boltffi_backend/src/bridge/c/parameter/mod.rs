mod byte_slice;
mod closure;
mod continuation;
mod group;

use crate::core::Result;

use boltffi_binding::ClosureSignature;

use super::{Identifier, Type};

pub use byte_slice::ByteSliceParameter;
pub use closure::ClosureParameter;
pub use continuation::ContinuationParameter;
pub use group::ParameterGroup;

/// A C function parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Parameter {
    name: Identifier,
    ty: Type,
    role: ParameterRole,
}

/// Position of a C ABI parameter in a function declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct ParameterIndex {
    index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParameterRole {
    Value,
    BytePointer(Identifier),
    ByteLength(Identifier),
    ContinuationData(Identifier),
    ContinuationCallback(Identifier),
    ClosureCall {
        name: Identifier,
        signature: ClosureSignature,
    },
    ClosureContext(Identifier),
    ClosureRelease(Identifier),
}

impl Parameter {
    /// Creates a value C ABI parameter.
    pub fn new(name: impl Into<String>, ty: Type) -> Result<Self> {
        Self::with_role(name, ty, ParameterRole::Value)
    }

    /// Creates the pointer half of a borrowed byte-slice C ABI parameter group.
    pub fn byte_pointer(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_ptr"),
            Type::ConstPointer(Box::new(Type::Uint8)),
            ParameterRole::BytePointer(Identifier::escape(name)?),
        )
    }

    /// Creates the length half of a borrowed byte-slice C ABI parameter group.
    pub fn byte_length(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_len"),
            Type::PointerWidth,
            ParameterRole::ByteLength(Identifier::escape(name)?),
        )
    }

    /// Creates the data half of a poll continuation C ABI parameter group.
    pub fn continuation_data(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_data"),
            Type::Uint64,
            ParameterRole::ContinuationData(Identifier::escape(name)?),
        )
    }

    /// Creates the function pointer half of a poll continuation C ABI parameter group.
    pub fn continuation_callback(name: &str, result: Type) -> Result<Self> {
        Self::with_role(
            name,
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::Uint64, result],
            },
            ParameterRole::ContinuationCallback(Identifier::escape(name)?),
        )
    }

    /// Creates the call function pointer in a closure C ABI parameter group.
    pub fn closure_call(name: &str, signature: &ClosureSignature, ty: Type) -> Result<Self> {
        Self::with_role(
            format!("{name}_call"),
            ty,
            ParameterRole::ClosureCall {
                name: Identifier::escape(name)?,
                signature: signature.clone(),
            },
        )
    }

    /// Creates the context pointer in a closure C ABI parameter group.
    pub fn closure_context(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_context"),
            Type::MutPointer(Box::new(Type::Void)),
            ParameterRole::ClosureContext(Identifier::escape(name)?),
        )
    }

    /// Creates the release function pointer in a closure C ABI parameter group.
    pub fn closure_release(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_release"),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::MutPointer(Box::new(Type::Void))],
            },
            ParameterRole::ClosureRelease(Identifier::escape(name)?),
        )
    }

    /// Returns the parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the parameter type.
    pub fn ty(&self) -> &Type {
        &self.ty
    }

    fn with_role(name: impl Into<String>, ty: Type, role: ParameterRole) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(name)?,
            ty,
            role,
        })
    }
}

impl ParameterIndex {
    /// Returns the zero-based C ABI parameter position.
    pub const fn position(self) -> usize {
        self.index
    }

    const fn new(index: usize) -> Self {
        Self { index }
    }
}

impl ParameterRole {
    fn is_byte_length(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ByteLength(name) if name == expected)
    }

    fn is_continuation_callback(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ContinuationCallback(name) if name == expected)
    }

    fn is_closure_context(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ClosureContext(name) if name == expected)
    }

    fn is_closure_release(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ClosureRelease(name) if name == expected)
    }
}

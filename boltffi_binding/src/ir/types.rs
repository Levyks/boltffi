use boltffi_ast::BuiltinType;
use serde::{Deserialize, Serialize};

use crate::{CallbackId, ClassId, CustomTypeId, EnumId, Primitive, RecordId, StreamId};

/// The value a binding declaration accepts or returns.
///
/// Higher-level than [`Primitive`]: covers the heap-managed primitives
/// the contract treats specially (`String`, `Bytes`), references to
/// user-declared types (`Record`, `Enum`, `Class`, `Callback`, `Custom`),
/// and the container shapes (`Optional`, `Sequence`, `Tuple`, `Result`, `Map`).
///
/// Source spelling is gone by the time a value reaches `TypeRef`. A Rust
/// `Option<Vec<UserProfile>>` is represented as
/// `Optional(Sequence(Record(id_of_user_profile)))`; whether it renders as
/// `[UserProfile]?` in Swift or `list[UserProfile] | None` in Python is a
/// later decision.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TypeRef {
    /// Primitive scalar value.
    Primitive(Primitive),
    /// UTF-8 string value.
    String,
    /// Byte buffer value.
    Bytes,
    /// Record reference.
    Record(RecordId),
    /// Enum reference.
    Enum(EnumId),
    /// Class reference.
    Class(ClassId),
    /// Callback reference.
    Callback(CallbackId),
    /// Custom type reference.
    Custom(CustomTypeId),
    /// Builtin value reference.
    Builtin(BuiltinType),
    /// Optional value.
    Optional(Box<TypeRef>),
    /// Sequence value.
    Sequence(Box<TypeRef>),
    /// Tuple value.
    Tuple(Vec<TypeRef>),
    /// Fallible value.
    Result {
        /// Success type.
        ok: Box<TypeRef>,
        /// Error type.
        err: Box<TypeRef>,
    },
    /// Map value.
    Map {
        /// Key type.
        key: Box<TypeRef>,
        /// Value type.
        value: Box<TypeRef>,
    },
}

/// The result type of a callable, including the absence of a result.
///
/// `()` is meaningful in a return position and meaningless as a field or
/// parameter type, so a separate wrapper keeps the latter from accepting a
/// "void" value.
///
/// # Example
///
/// `ReturnTypeRef::Void` for `fn save() -> ()`,
/// `ReturnTypeRef::Value(TypeRef::Primitive(Primitive::I32))` for
/// `fn count() -> i32`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ReturnTypeRef {
    /// The callable returns no value.
    Void,
    /// The callable returns one value.
    Value(TypeRef),
}

/// What an opaque handle stands in for.
///
/// Handles cross the boundary as integer tokens; the variants name the
/// kinds of declarations a token can refer to. Excludes value-shaped
/// types like primitives, records, and enums, which never cross as
/// opaque tokens. Narrower than [`TypeRef`] so the type system rejects
/// "handle to `i32`" or "handle to `Point`" at the construction site.
///
/// # Example
///
/// A `Class` handle into a Rust-owned `Engine` instance is represented
/// as `HandleTarget::Class(engine_id)`. A foreign-implemented callback
/// trait is `HandleTarget::Callback(listener_id)`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HandleTarget {
    /// Class instance owned by Rust.
    Class(ClassId),
    /// Callback object implemented on the foreign side.
    Callback(CallbackId),
    /// Stream of values produced by Rust.
    Stream(StreamId),
}

/// Whether a handle-typed slot is always populated or may be absent.
///
/// Nullability is a per-site decision on the dispatch plan, not a
/// property of the target type. The same callback trait can be required
/// on one method and nullable on another. The wire shape is identical;
/// the carrier is the same width and a zero/null sentinel encodes the
/// absent state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum HandlePresence {
    /// Caller must supply a live handle.
    Required,
    /// Caller may omit the handle; a zero/null sentinel encodes absence.
    Nullable,
}

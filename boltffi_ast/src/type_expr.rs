use serde::{Deserialize, Serialize};

use crate::{ClassId, CustomTypeId, EnumId, Primitive, RecordId, ReturnDef, TraitId};

/// Form in which a Rust trait appears as a source value.
///
/// Names the supported Rust spellings for trait-typed values: a
/// monomorphized `impl Trait`, an owned `Box<dyn Trait>`, or a shared
/// `Arc<dyn Trait>`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TraitUseForm {
    /// `impl Trait`.
    ImplTrait,
    /// `Box<dyn Trait>`.
    BoxedDyn,
    /// `Arc<dyn Trait>`.
    ArcDyn,
}

/// Whether a handle-typed source value is required or optional.
///
/// The scanner folds source shapes such as `Option<Engine>` and
/// `Option<Box<dyn Listener>>` into the handle-bearing type expression
/// instead of keeping an outer [`TypeExpr::Option`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum HandlePresence {
    /// Caller must supply a live handle.
    Required,
    /// Caller may omit the handle; a zero/null sentinel encodes absence.
    Nullable,
}

/// A type expression in the exported Rust surface.
///
/// This is the shape produced after scanning a Rust type from a field,
/// parameter, or return. Known exported names have been turned into IDs, and
/// ordinary Rust containers remain as a tree. For example,
/// `Option<Vec<Point>>` becomes `Option(Vec(Record(point_id)))`, `(u32,
/// String)` becomes `Tuple([Primitive(U32), String])`, inline closure
/// signatures become `Closure`, and `Self` stays explicit when it appears
/// inside an impl block.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    /// A primitive Rust scalar.
    Primitive(Primitive),
    /// The Rust unit type `()`.
    ///
    /// A callable that returns nothing uses
    /// [`ReturnDef::Void`](crate::ReturnDef::Void). This variant records
    /// unit when it appears as a written value type, such as
    /// `Result<(), E>`.
    Unit,
    /// A record declaration by ID.
    Record(RecordId),
    /// An enum declaration by ID.
    Enum(EnumId),
    /// A class-style object reference.
    ///
    /// Source `Option<Engine>` is represented as `Class { id, presence:
    /// Nullable }`; required class values use `presence: Required`.
    Class {
        /// The class declaration this reference resolves to.
        id: ClassId,
        /// Whether the source value is nullable.
        presence: HandlePresence,
    },
    /// A reference to a Rust trait the user wrote.
    ///
    /// The source entity is a trait, such as `trait Listener { ... }`,
    /// and this variant references it. Source `Option<Box<dyn T>>`
    /// collapses to `Trait { form: BoxedDyn, presence: Nullable }`.
    Trait {
        /// The trait declaration this reference resolves to.
        id: TraitId,
        /// Rust form used at this trait-typed source value.
        form: TraitUseForm,
        /// Whether the source value is nullable.
        presence: HandlePresence,
    },
    /// An inline closure signature such as `impl Fn(u32) -> String`.
    Closure(Box<ClosureType>),
    /// A custom type declaration by ID.
    Custom(CustomTypeId),
    /// The Rust `Self` type inside an impl, trait, or callback context.
    SelfType,
    /// A `Vec<T>` source type.
    Vec(Box<TypeExpr>),
    /// An `Option<T>` source type.
    ///
    /// Used for ordinary optional values, such as `Option<i32>` and
    /// `Option<String>`. Source `Option<...>` wrapping a supported
    /// trait-typed value lands in [`TypeExpr::Trait`] with
    /// `presence: Nullable`.
    Option(Box<TypeExpr>),
    /// A `Result<T, E>` source type.
    ///
    /// Preserves both source type arguments in order. The same expression
    /// can appear in returns, fields, parameters, or nested containers.
    Result {
        /// Success type written as the first `Result` argument.
        ok: Box<TypeExpr>,
        /// Error type written as the second `Result` argument.
        err: Box<TypeExpr>,
    },
    /// A tuple type such as `(u32, String)`.
    ///
    /// Tuples are ordinary value types in the AST. A function returning
    /// `(u32, String)` is represented as `ReturnDef::Value(TypeExpr::Tuple(_))`,
    /// while a function returning `Result<(u32, String), Error>` is represented
    /// as `ReturnDef::Value(TypeExpr::Result { ok: TypeExpr::Tuple(_), err: ... })`.
    /// The empty tuple is *not* represented here; use [`TypeExpr::Unit`].
    Tuple(Vec<TypeExpr>),
    /// A map-like source type.
    Map {
        /// Key type written by the source map.
        key: Box<TypeExpr>,
        /// Value type written by the source map.
        value: Box<TypeExpr>,
    },
    /// A UTF-8 string source type.
    String,
    /// A byte buffer source type.
    Bytes,
    /// A type parameter used by a generic declaration the scanner chose to keep.
    Parameter(TypeParameter),
}

impl TypeExpr {
    /// Builds a `Vec<T>` type expression.
    ///
    /// The `element` parameter is the source type written inside the vector.
    ///
    /// Returns a vector type expression.
    pub fn vec(element: TypeExpr) -> Self {
        Self::Vec(Box::new(element))
    }

    /// Builds an `Option<T>` type expression.
    ///
    /// The `inner` parameter is the source type written inside the option.
    ///
    /// Returns an optional type expression.
    pub fn option(inner: TypeExpr) -> Self {
        Self::Option(Box::new(inner))
    }

    /// Builds a `Result<T, E>` type expression.
    ///
    /// The `ok` parameter is the success type. The `err` parameter is the error
    /// type.
    ///
    /// Returns a result type expression for nested or non-callable positions.
    pub fn result(ok: TypeExpr, err: TypeExpr) -> Self {
        Self::Result {
            ok: Box::new(ok),
            err: Box::new(err),
        }
    }

    /// Builds an inline closure type expression.
    ///
    /// The `closure` parameter contains the callable signature written inside a
    /// closure-like parameter type.
    ///
    /// Returns a closure type expression.
    pub fn closure(closure: ClosureType) -> Self {
        Self::Closure(Box::new(closure))
    }

    /// Builds a trait-reference type expression.
    ///
    /// The `id` parameter is the trait declaration. The `form` parameter is
    /// the Rust form used in source. The `presence` parameter is the
    /// source nullability.
    pub fn r#trait(id: TraitId, form: TraitUseForm, presence: HandlePresence) -> Self {
        Self::Trait { id, form, presence }
    }

    /// Builds a class-reference type expression.
    ///
    /// The `id` parameter is the class declaration. The `presence`
    /// parameter is the source nullability.
    pub fn class(id: ClassId, presence: HandlePresence) -> Self {
        Self::Class { id, presence }
    }

    /// Builds a tuple type expression.
    ///
    /// The `elements` parameter preserves the tuple element types in source
    /// order. A one-element tuple still has one element here; the scanner does
    /// not need a special case for Rust's trailing-comma syntax once parsing is
    /// finished.
    ///
    /// Returns a tuple value type, suitable for fields, parameters, nested
    /// containers, and `ReturnDef::Value`.
    pub fn tuple(elements: Vec<TypeExpr>) -> Self {
        Self::Tuple(elements)
    }

    /// Builds a map type expression.
    ///
    /// The `key` parameter is the source key type. The `value` parameter is the
    /// source value type.
    ///
    /// Returns a map type expression.
    pub fn map(key: TypeExpr, value: TypeExpr) -> Self {
        Self::Map {
            key: Box::new(key),
            value: Box::new(value),
        }
    }
}

/// Kind of inline closure-shaped parameter the source wrote.
///
/// Mirrors what the Rust parser saw. The `FunctionPointer` variant is
/// the bare `fn(...)` type (no captured environment); `Fn` / `FnMut` /
/// `FnOnce` are the standard Rust closure trait flavors used as
/// `impl Fn*(...)` parameter bounds.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ClosureKind {
    /// Bare `fn(...)` function-pointer parameter.
    FunctionPointer,
    /// `impl Fn(...)` parameter.
    Fn,
    /// `impl FnMut(...)` parameter.
    FnMut,
    /// `impl FnOnce(...)` parameter.
    FnOnce,
}

/// An inline closure signature used as a type expression.
///
/// Closure parameters are not named declarations in Rust source. The scanner
/// stores the source `kind` (function pointer vs `Fn` family), the parameter
/// list, and the return type here so the closure shape remains local to the
/// parameter that introduced it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClosureType {
    /// Source-form kind the parser saw.
    pub kind: ClosureKind,
    /// Types accepted by the closure in source order.
    pub parameters: Vec<TypeExpr>,
    /// Return type written by the closure signature.
    pub returns: ReturnDef,
}

impl ClosureType {
    /// Builds an inline closure signature.
    ///
    /// The `kind` parameter records the source spelling the parser saw.
    /// The `parameters` parameter preserves closure parameter types in source order.
    /// The `returns` parameter is the closure return type.
    ///
    /// Returns a closure signature suitable for [`TypeExpr::Closure`].
    pub fn new(kind: ClosureKind, parameters: Vec<TypeExpr>, returns: ReturnDef) -> Self {
        Self {
            kind,
            parameters,
            returns,
        }
    }
}

/// A named type parameter referenced by a source type expression.
///
/// Generic exports may be rejected or specialized after scanning. Preserving
/// the parameter name gives those errors the original source shape.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TypeParameter {
    /// Parameter name as written in Rust source.
    pub name: String,
}

impl TypeParameter {
    /// Builds a type parameter reference.
    ///
    /// The `name` parameter is stored exactly as the scanner reported it.
    ///
    /// Returns a type parameter expression for generic source syntax.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

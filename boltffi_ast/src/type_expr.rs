use serde::{Deserialize, Serialize};

use crate::{CallbackId, ClassId, CustomTypeId, EnumId, Primitive, RecordId, ReturnDef};

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
    /// A record declaration by ID.
    Record(RecordId),
    /// An enum declaration by ID.
    Enum(EnumId),
    /// A class-style object declaration by ID.
    Class(ClassId),
    /// A callback trait declaration by ID.
    Callback(CallbackId),
    /// An inline closure signature such as `impl Fn(u32) -> String`.
    Closure(Box<ClosureType>),
    /// A custom type declaration by ID.
    Custom(CustomTypeId),
    /// The Rust `Self` type inside an impl, trait, or callback context.
    SelfType,
    /// A `Vec<T>` source type.
    Vec(Box<TypeExpr>),
    /// An `Option<T>` source type.
    Option(Box<TypeExpr>),
    /// A `Result<T, E>` source type.
    ///
    /// In return position, lowering treats this as a success type plus an error
    /// channel. In field and parameter position, it remains an ordinary value
    /// type.
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
    /// Returns a closure type expression that can be paired with
    /// [`ParameterPassing::ImplTrait`](crate::ParameterPassing::ImplTrait) or
    /// [`ParameterPassing::BoxedDyn`](crate::ParameterPassing::BoxedDyn).
    pub fn closure(closure: ClosureType) -> Self {
        Self::Closure(Box::new(closure))
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

/// An inline closure signature used as a type expression.
///
/// Closure parameters are not named declarations in Rust source. The scanner
/// stores their parameter and return types here so the callback shape remains
/// local to the parameter that introduced it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClosureType {
    /// Types accepted by the closure in source order.
    pub parameters: Vec<TypeExpr>,
    /// Return type written by the closure signature.
    pub returns: ReturnDef,
}

impl ClosureType {
    /// Builds an inline closure signature.
    ///
    /// The `parameters` parameter preserves closure parameter types in source order.
    /// The `returns` parameter is the closure return type.
    ///
    /// Returns a closure signature suitable for [`TypeExpr::Closure`].
    pub fn new(parameters: Vec<TypeExpr>, returns: ReturnDef) -> Self {
        Self {
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

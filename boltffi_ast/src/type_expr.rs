use serde::{Deserialize, Serialize};

use crate::{ClassId, CustomTypeId, EnumId, Path, Primitive, RecordId, ReturnDef, TraitId};

/// A Rust type exactly as written at an exported BoltFFI use site.
///
/// Every variant mirrors a Rust type form and nests like the source, so
/// `Option<Arc<dyn Listener>>` is `Option(Arc(Dyn(Trait)))` rather than one
/// folded nullable-callback shape. Faithfulness is the contract: the
/// expression records what the source says, not what the FFI boundary needs.
/// Whether a representable type is supported in the position where it appears
/// is decided by later stages.
///
/// Invalid Rust stays unrepresentable. `dyn` and `impl Trait` accept only a
/// [`TraitBound`], so `dyn Vec<u8>` cannot be constructed.
///
/// Named leaves keep both a stable `id` and the `path` as written, because the
/// canonical identity (`demo::Point`) and the source spelling
/// (`crate::geometry::Point`) can differ and regenerated Rust must reproduce
/// the spelling.
///
/// # Example
///
/// The parameter of `fn open(engine: Option<Engine>)` is
/// `Option(Class { id, path })`, and `fn bytes(data: Vec<u8>)` is
/// `Vec(Primitive(U8))`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TypeExpr {
    /// A scalar such as `u32` or `bool`.
    Primitive(Primitive),
    /// The unit type `()`.
    Unit,
    /// The owned `String` type.
    String,
    /// The borrowed `str` type.
    Str,
    /// A struct exported as a BoltFFI record.
    Record {
        /// Declaration this reference resolves to.
        id: RecordId,
        /// Path as written at this use site.
        path: Path,
    },
    /// An enum exported as a BoltFFI enum.
    Enum {
        /// Declaration this reference resolves to.
        id: EnumId,
        /// Path as written at this use site.
        path: Path,
    },
    /// A type exported as a BoltFFI class handle.
    Class {
        /// Declaration this reference resolves to.
        id: ClassId,
        /// Path as written at this use site.
        path: Path,
    },
    /// A type bridged through a custom converter.
    Custom {
        /// Declaration this reference resolves to.
        id: CustomTypeId,
        /// Path as written at this use site.
        path: Path,
    },
    /// A `dyn Trait` object.
    Dyn(TraitBound),
    /// An `impl Trait`.
    ImplTrait(TraitBound),
    /// A `Box<T>`.
    Boxed(Box<TypeExpr>),
    /// An `Arc<T>`.
    Arc(Box<TypeExpr>),
    /// A function pointer such as `fn(u32) -> bool`.
    FnPtr(Box<FnSig>),
    /// A `Vec<T>`.
    Vec(Box<TypeExpr>),
    /// A slice `[T]`.
    Slice(Box<TypeExpr>),
    /// An `Option<T>`.
    Option(Box<TypeExpr>),
    /// A `Result<T, E>`.
    Result {
        /// Success type, the first `Result` argument.
        ok: Box<TypeExpr>,
        /// Error type, the second `Result` argument.
        err: Box<TypeExpr>,
    },
    /// A tuple such as `(u32, String)`.
    Tuple(Vec<TypeExpr>),
    /// A `HashMap` or `BTreeMap`.
    Map {
        /// Which map constructor was written.
        kind: MapKind,
        /// Key type written between the angle brackets.
        key: Box<TypeExpr>,
        /// Value type written between the angle brackets.
        value: Box<TypeExpr>,
    },
    /// The `Self` type.
    SelfType,
    /// A named type parameter such as `T`.
    Parameter(TypeParameter),
}

impl TypeExpr {
    /// Builds a resolved record type.
    pub fn record(id: RecordId, path: Path) -> Self {
        Self::Record { id, path }
    }

    /// Builds a resolved enum type.
    pub fn enumeration(id: EnumId, path: Path) -> Self {
        Self::Enum { id, path }
    }

    /// Builds a resolved class type.
    pub fn class(id: ClassId, path: Path) -> Self {
        Self::Class { id, path }
    }

    /// Builds a resolved custom type.
    pub fn custom(id: CustomTypeId, path: Path) -> Self {
        Self::Custom { id, path }
    }

    /// Builds a `dyn Trait` expression for a declared callback trait.
    pub fn dyn_trait(id: TraitId, path: Path) -> Self {
        Self::Dyn(TraitBound::Trait { id, path })
    }

    /// Builds an `impl Trait` expression for a declared callback trait.
    pub fn impl_trait(id: TraitId, path: Path) -> Self {
        Self::ImplTrait(TraitBound::Trait { id, path })
    }

    /// Builds a `dyn Fn*` expression.
    pub fn dyn_fn(function_trait: FnTrait) -> Self {
        Self::Dyn(TraitBound::Fn(Box::new(function_trait)))
    }

    /// Builds an `impl Fn*` expression.
    pub fn impl_fn(function_trait: FnTrait) -> Self {
        Self::ImplTrait(TraitBound::Fn(Box::new(function_trait)))
    }

    /// Builds a `Box<T>` expression.
    pub fn boxed(inner: TypeExpr) -> Self {
        Self::Boxed(Box::new(inner))
    }

    /// Builds an `Arc<T>` expression.
    pub fn arc(inner: TypeExpr) -> Self {
        Self::Arc(Box::new(inner))
    }

    /// Builds a bare Rust function pointer expression.
    pub fn fn_ptr(signature: FnSig) -> Self {
        Self::FnPtr(Box::new(signature))
    }

    /// Builds a `Vec<T>` expression.
    pub fn vec(element: TypeExpr) -> Self {
        Self::Vec(Box::new(element))
    }

    /// Builds a slice expression.
    pub fn slice(element: TypeExpr) -> Self {
        Self::Slice(Box::new(element))
    }

    /// Builds an `Option<T>` expression.
    pub fn option(inner: TypeExpr) -> Self {
        Self::Option(Box::new(inner))
    }

    /// Builds a `Result<T, E>` expression.
    pub fn result(ok: TypeExpr, err: TypeExpr) -> Self {
        Self::Result {
            ok: Box::new(ok),
            err: Box::new(err),
        }
    }

    /// Builds a tuple expression.
    pub fn tuple(elements: Vec<TypeExpr>) -> Self {
        Self::Tuple(elements)
    }

    /// Builds a `HashMap<K, V>` expression.
    pub fn hash_map(key: TypeExpr, value: TypeExpr) -> Self {
        Self::map(MapKind::Hash, key, value)
    }

    /// Builds a `BTreeMap<K, V>` expression.
    pub fn btree_map(key: TypeExpr, value: TypeExpr) -> Self {
        Self::map(MapKind::BTree, key, value)
    }

    /// Builds a map-like expression.
    pub fn map(kind: MapKind, key: TypeExpr, value: TypeExpr) -> Self {
        Self::Map {
            kind,
            key: Box::new(key),
            value: Box::new(value),
        }
    }
}

/// A trait that can sit behind `dyn` or `impl Trait`.
///
/// Only a trait is legal in those positions, so restricting [`TypeExpr::Dyn`]
/// and [`TypeExpr::ImplTrait`] to this type keeps `dyn Vec<u8>` and
/// `impl Point` unrepresentable.
///
/// # Example
///
/// `Box<dyn Listener>` carries `Trait { id, path }`, while
/// `Box<dyn Fn(u32)>` carries `Fn(_)`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum TraitBound {
    /// A trait exported as a BoltFFI callback.
    Trait {
        /// Declaration this reference resolves to.
        id: TraitId,
        /// Path as written at this use site.
        path: Path,
    },
    /// An `Fn`, `FnMut`, or `FnOnce` bound.
    Fn(Box<FnTrait>),
}

/// Which map constructor was written.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum MapKind {
    /// `HashMap<K, V>`.
    Hash,
    /// `BTreeMap<K, V>`.
    BTree,
}

/// An `Fn`, `FnMut`, or `FnOnce` bound carrying its call signature.
///
/// The bound kind and the signature together fix how a closure value is
/// called, so a borrowing `Fn` is distinct from a consuming `FnOnce` even when
/// their parameters and result match.
///
/// # Example
///
/// `FnMut(u32) -> bool` is `FnTrait { kind: FnMut, signature }` whose signature
/// has `parameters` `[Primitive(U32)]` and `returns` `Value(Primitive(Bool))`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FnTrait {
    /// Which of `Fn`, `FnMut`, or `FnOnce` was written.
    pub kind: FnTraitKind,
    /// Parameter and return types between the parentheses.
    pub signature: FnSig,
}

impl FnTrait {
    /// Builds an `Fn`-family bound.
    pub fn new(kind: FnTraitKind, signature: FnSig) -> Self {
        Self { kind, signature }
    }
}

/// One of the `Fn`, `FnMut`, or `FnOnce` traits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum FnTraitKind {
    /// `Fn`.
    Fn,
    /// `FnMut`.
    FnMut,
    /// `FnOnce`.
    FnOnce,
}

impl AsRef<str> for FnTraitKind {
    fn as_ref(&self) -> &str {
        match self {
            Self::Fn => "Fn",
            Self::FnMut => "FnMut",
            Self::FnOnce => "FnOnce",
        }
    }
}

/// The call shape shared by function pointers and `Fn`-family bounds.
///
/// A signature is the parameter list and result of anything callable, so the
/// same value describes `fn(u32) -> bool` under [`TypeExpr::FnPtr`] and
/// `Fn(u32) -> bool` under [`FnTrait`]. A signature with no written return
/// uses [`ReturnDef::Void`](crate::ReturnDef::Void) rather than a unit type.
///
/// # Example
///
/// `Fn(u32, &str) -> bool` is a signature whose `parameters` are
/// `[Primitive(U32), Str]` and whose `returns` is `Value(Primitive(Bool))`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct FnSig {
    /// Parameter types in source order.
    pub parameters: Vec<TypeExpr>,
    /// The result, void when the signature writes no return.
    pub returns: ReturnDef,
}

impl FnSig {
    /// Builds a call signature.
    pub fn new(parameters: Vec<TypeExpr>, returns: ReturnDef) -> Self {
        Self {
            parameters,
            returns,
        }
    }
}

/// A generic type parameter such as `T`, kept by its source name.
///
/// A parameter resolves to no concrete type, so later stages reject it where a
/// value type is required. Retaining the written name lets that rejection name
/// the parameter the source used.
///
/// # Example
///
/// The `T` in an exported `fn first<T>(items: Vec<T>) -> T` is
/// `Parameter(TypeParameter { name: "T" })`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct TypeParameter {
    /// Parameter name as written in source.
    pub name: String,
}

impl TypeParameter {
    /// Builds a type parameter from its source name.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

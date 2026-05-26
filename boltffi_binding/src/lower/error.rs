use std::{error::Error, fmt};

use crate::BindingError;

/// An error returned while lowering source into a binding contract.
///
/// Lowering fails before a [`Bindings`](crate::Bindings) value exists.
/// Failures here describe source shapes the lowering pass cannot represent
/// yet, unresolved declaration references, or invalid binding values
/// rejected during final validation.
#[derive(Debug)]
pub struct LowerError {
    kind: LowerErrorKind,
}

impl LowerError {
    pub(crate) fn new(kind: LowerErrorKind) -> Self {
        Self { kind }
    }

    pub(crate) fn unsupported_declaration(family: DeclarationFamily) -> Self {
        Self::new(LowerErrorKind::UnsupportedDeclaration(family))
    }

    pub(crate) fn unsupported_type(unsupported: UnsupportedType) -> Self {
        Self::new(LowerErrorKind::UnsupportedType(unsupported))
    }

    pub(crate) fn duplicate_source_id(family: DeclarationFamily, id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::DuplicateSourceId {
            family,
            id: id.to_string(),
        })
    }

    pub(crate) fn unknown_record(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownRecord(id.to_string()))
    }

    pub(crate) fn unknown_enum(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownEnum(id.to_string()))
    }

    pub(crate) fn unknown_class(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownClass(id.to_string()))
    }

    pub(crate) fn unknown_callback(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownCallback(id.to_string()))
    }

    pub(crate) fn unknown_custom(id: impl fmt::Display) -> Self {
        Self::new(LowerErrorKind::UnknownCustom(id.to_string()))
    }

    pub(crate) fn invalid_alignment(bytes: u64) -> Self {
        Self::new(LowerErrorKind::InvalidAlignment(bytes))
    }

    pub(crate) fn discriminant_overflow() -> Self {
        Self::new(LowerErrorKind::DiscriminantOverflow)
    }

    pub(crate) fn variant_tag_overflow() -> Self {
        Self::new(LowerErrorKind::VariantTagOverflow)
    }

    pub(crate) fn field_position_overflow() -> Self {
        Self::new(LowerErrorKind::FieldPositionOverflow)
    }

    /// Returns the reason lowering failed.
    pub fn kind(&self) -> &LowerErrorKind {
        &self.kind
    }
}

impl fmt::Display for LowerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            LowerErrorKind::UnsupportedDeclaration(family) => {
                write!(formatter, "{} lowering is not implemented", family)
            }
            LowerErrorKind::UnsupportedType(unsupported) => {
                write!(
                    formatter,
                    "{} cannot be represented in binding IR yet",
                    unsupported
                )
            }
            LowerErrorKind::DuplicateSourceId { family, id } => {
                write!(formatter, "duplicate {} source id `{id}`", family)
            }
            LowerErrorKind::UnknownRecord(record) => {
                write!(formatter, "unknown record id `{record}`")
            }
            LowerErrorKind::UnknownEnum(enumeration) => {
                write!(formatter, "unknown enum id `{enumeration}`")
            }
            LowerErrorKind::UnknownClass(class) => {
                write!(formatter, "unknown class id `{class}`")
            }
            LowerErrorKind::UnknownCallback(callback) => {
                write!(formatter, "unknown callback id `{callback}`")
            }
            LowerErrorKind::UnknownCustom(custom) => {
                write!(formatter, "unknown custom type id `{custom}`")
            }
            LowerErrorKind::InvalidAlignment(alignment) => {
                write!(formatter, "invalid record alignment {alignment}")
            }
            LowerErrorKind::DiscriminantOverflow => {
                formatter.write_str("enum discriminant overflow")
            }
            LowerErrorKind::VariantTagOverflow => formatter.write_str("enum variant tag overflow"),
            LowerErrorKind::FieldPositionOverflow => formatter.write_str("field position overflow"),
            LowerErrorKind::InvalidBindings(error) => error.fmt(formatter),
        }
    }
}

impl Error for LowerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            LowerErrorKind::InvalidBindings(error) => Some(error),
            _ => None,
        }
    }
}

/// The reason source lowering failed.
///
/// Variants are precise enough for diagnostics to point at the unsupported
/// declaration family, unresolved reference, or invalid value that stopped
/// construction.
#[derive(Debug)]
#[non_exhaustive]
pub enum LowerErrorKind {
    /// A top-level declaration family is not lowered by this slice yet.
    UnsupportedDeclaration(DeclarationFamily),
    /// A source type has no binding-IR representation yet.
    UnsupportedType(UnsupportedType),
    /// Two declarations in the same family share one source id.
    DuplicateSourceId {
        /// Declaration family where the duplicate was found.
        family: DeclarationFamily,
        /// Duplicated source id.
        id: String,
    },
    /// A record reference could not be resolved inside the source contract.
    UnknownRecord(String),
    /// An enum reference could not be resolved inside the source contract.
    UnknownEnum(String),
    /// A class reference could not be resolved inside the source contract.
    UnknownClass(String),
    /// A callback reference could not be resolved inside the source contract.
    UnknownCallback(String),
    /// A custom type reference could not be resolved inside the source contract.
    UnknownCustom(String),
    /// A computed record alignment was not a valid ABI alignment.
    InvalidAlignment(u64),
    /// An enum discriminant sequence overflowed `i128`.
    DiscriminantOverflow,
    /// A data enum variant index could not fit in a variant tag.
    VariantTagOverflow,
    /// A tuple field index could not fit in a field position.
    FieldPositionOverflow,
    /// The lowered contract failed binding validation.
    InvalidBindings(BindingError),
}

/// A declaration family known to the lowering pass.
///
/// Used in diagnostics when an entire family is unsupported or contains
/// duplicate source IDs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DeclarationFamily {
    /// Record declarations.
    Records,
    /// Enum declarations.
    Enums,
    /// Free function declarations.
    Functions,
    /// Class-style object declarations.
    Classes,
    /// Callback trait declarations.
    CallbackTraits,
    /// Stream declarations.
    Streams,
    /// Constant declarations.
    Constants,
    /// Custom type declarations.
    CustomTypes,
    /// Methods attached to records.
    RecordMethods,
    /// Methods attached to enums.
    EnumMethods,
}

impl fmt::Display for DeclarationFamily {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Records => "records",
            Self::Enums => "enums",
            Self::Functions => "functions",
            Self::Classes => "classes",
            Self::CallbackTraits => "callback traits",
            Self::Streams => "streams",
            Self::Constants => "constants",
            Self::CustomTypes => "custom types",
            Self::RecordMethods => "record methods",
            Self::EnumMethods => "enum methods",
        })
    }
}

/// A source type shape the lowering pass cannot represent yet.
///
/// These are gaps in the current binding-IR slice, not generic parse errors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum UnsupportedType {
    /// A direct record field was not a fixed-width primitive.
    RecordField,
    /// An enum representation did not resolve to an integer carrier.
    EnumRepr,
    /// `Self` appeared where the lowering pass did not have an owning declaration.
    SelfType,
    /// A generic type parameter appeared in exported source.
    TypeParameter,
    /// A closure returned `Result<T, E>`.
    FallibleClosureReturn,
    /// A default value cannot be emitted as binding metadata yet.
    DefaultValue,
    /// An `async` callable cannot be lowered yet.
    AsyncCallable,
    /// An `impl Trait` parameter has no IR slice yet.
    ImplTraitParameter,
    /// A `Box<dyn Trait>` parameter has no IR slice yet.
    BoxedDynParameter,
    /// An owned class receiver has no handle-transfer protocol yet.
    OwnedClassReceiver,
}

impl fmt::Display for UnsupportedType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RecordField => "record field",
            Self::EnumRepr => "enum repr",
            Self::SelfType => "Self",
            Self::TypeParameter => "type parameter",
            Self::FallibleClosureReturn => "fallible closure return",
            Self::DefaultValue => "default value",
            Self::AsyncCallable => "async callable",
            Self::ImplTraitParameter => "impl Trait parameter",
            Self::BoxedDynParameter => "Box<dyn Trait> parameter",
            Self::OwnedClassReceiver => "owned class receiver",
        })
    }
}

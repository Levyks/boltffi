use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanError {
    Read {
        path: String,
        message: String,
    },
    Parse {
        path: String,
        message: String,
    },
    ModuleNotFound {
        module: String,
        searched: Vec<String>,
    },
    UnsupportedType {
        spelling: String,
    },
    InvalidMarker {
        attribute: String,
    },
    InvalidMarkerPlacement {
        marker: String,
        item: String,
    },
    ConflictingMarkers {
        first: String,
        second: String,
    },
    UnsupportedMarkedImpl {
        target: String,
    },
    UnsupportedGenerics {
        item: String,
    },
    UnsupportedUnsafe {
        item: String,
    },
    UnsupportedExternAbi {
        item: String,
    },
    UnsupportedSupertraits {
        item: String,
    },
    UnsupportedTraitItem {
        item: String,
    },
    UnsupportedTraitMethodBody {
        item: String,
    },
    UnnamedParameter,
    ReceiverOnFreeFunction,
    TupleOrUnitStruct,
    UnsupportedDiscriminant,
}

impl ScanError {
    pub(super) fn read(path: &std::path::Path, error: &std::io::Error) -> Self {
        Self::Read {
            path: path.display().to_string(),
            message: error.to_string(),
        }
    }

    pub(super) fn parse(path: &std::path::Path, error: &syn::Error) -> Self {
        Self::Parse {
            path: path.display().to_string(),
            message: error.to_string(),
        }
    }

    pub(super) fn unsupported_type(ty: &syn::Type) -> Self {
        Self::UnsupportedType {
            spelling: crate::spelling::ty(ty),
        }
    }
}

impl fmt::Display for ScanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, message } => {
                write!(formatter, "cannot read source file `{path}`: {message}")
            }
            Self::Parse { path, message } => {
                write!(formatter, "cannot parse source file `{path}`: {message}")
            }
            Self::ModuleNotFound { module, searched } => {
                write!(
                    formatter,
                    "cannot find module `{module}`, looked for {}",
                    searched.join(", ")
                )
            }
            Self::UnsupportedType { spelling } => {
                write!(formatter, "unsupported source type `{spelling}`")
            }
            Self::InvalidMarker { attribute } => {
                write!(formatter, "invalid BoltFFI marker `{attribute}`")
            }
            Self::InvalidMarkerPlacement { marker, item } => {
                write!(
                    formatter,
                    "BoltFFI marker `{marker}` cannot be used on `{item}`"
                )
            }
            Self::ConflictingMarkers { first, second } => {
                write!(
                    formatter,
                    "conflicting BoltFFI markers `{first}` and `{second}`"
                )
            }
            Self::UnsupportedMarkedImpl { target } => {
                write!(
                    formatter,
                    "marked impl target `{target}` is not a supported value type"
                )
            }
            Self::UnsupportedGenerics { item } => {
                write!(formatter, "`{item}` cannot use generics")
            }
            Self::UnsupportedUnsafe { item } => {
                write!(formatter, "`{item}` cannot be unsafe")
            }
            Self::UnsupportedExternAbi { item } => {
                write!(formatter, "`{item}` cannot declare an extern ABI")
            }
            Self::UnsupportedSupertraits { item } => {
                write!(formatter, "`{item}` cannot use supertraits")
            }
            Self::UnsupportedTraitItem { item } => {
                write!(formatter, "`{item}` is not supported in exported traits")
            }
            Self::UnsupportedTraitMethodBody { item } => {
                write!(formatter, "`{item}` cannot define a default body")
            }
            Self::UnnamedParameter => formatter.write_str("parameter pattern is not a plain name"),
            Self::ReceiverOnFreeFunction => {
                formatter.write_str("free function cannot have a receiver")
            }
            Self::TupleOrUnitStruct => {
                formatter.write_str("tuple and unit structs are not supported as records yet")
            }
            Self::UnsupportedDiscriminant => {
                formatter.write_str("enum discriminant is not an integer literal")
            }
        }
    }
}

impl std::error::Error for ScanError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn ty(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    #[test]
    fn unsupported_path_type_preserves_qualified_spelling() {
        assert_eq!(
            ScanError::unsupported_type(&ty("crate::domain::Point")),
            ScanError::UnsupportedType {
                spelling: "crate::domain::Point".to_owned()
            }
        );
    }

    #[test]
    fn unsupported_reference_type_preserves_reference_shape() {
        assert_eq!(
            ScanError::unsupported_type(&ty("&Point")),
            ScanError::UnsupportedType {
                spelling: "&Point".to_owned()
            }
        );
    }

    #[test]
    fn display_messages_are_stable_and_specific() {
        assert_eq!(
            ScanError::UnsupportedType {
                spelling: "Point".to_owned()
            }
            .to_string(),
            "unsupported source type `Point`"
        );
        assert_eq!(
            ScanError::UnnamedParameter.to_string(),
            "parameter pattern is not a plain name"
        );
        assert_eq!(
            ScanError::ReceiverOnFreeFunction.to_string(),
            "free function cannot have a receiver"
        );
        assert_eq!(
            ScanError::TupleOrUnitStruct.to_string(),
            "tuple and unit structs are not supported as records yet"
        );
        assert_eq!(
            ScanError::InvalidMarker {
                attribute: "data(foo)".to_owned()
            }
            .to_string(),
            "invalid BoltFFI marker `data(foo)`"
        );
        assert_eq!(
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "struct".to_owned()
            }
            .to_string(),
            "BoltFFI marker `export` cannot be used on `struct`"
        );
        assert_eq!(
            ScanError::ConflictingMarkers {
                first: "data".to_owned(),
                second: "error".to_owned()
            }
            .to_string(),
            "conflicting BoltFFI markers `data` and `error`"
        );
        assert_eq!(
            ScanError::UnsupportedMarkedImpl {
                target: "Missing".to_owned()
            }
            .to_string(),
            "marked impl target `Missing` is not a supported value type"
        );
        assert_eq!(
            ScanError::UnsupportedGenerics {
                item: "function make".to_owned()
            }
            .to_string(),
            "`function make` cannot use generics"
        );
        assert_eq!(
            ScanError::UnsupportedUnsafe {
                item: "function free".to_owned()
            }
            .to_string(),
            "`function free` cannot be unsafe"
        );
        assert_eq!(
            ScanError::UnsupportedExternAbi {
                item: "function add".to_owned()
            }
            .to_string(),
            "`function add` cannot declare an extern ABI"
        );
        assert_eq!(
            ScanError::UnsupportedSupertraits {
                item: "trait Listener".to_owned()
            }
            .to_string(),
            "`trait Listener` cannot use supertraits"
        );
        assert_eq!(
            ScanError::UnsupportedTraitItem {
                item: "trait Listener::Item".to_owned()
            }
            .to_string(),
            "`trait Listener::Item` is not supported in exported traits"
        );
        assert_eq!(
            ScanError::UnsupportedTraitMethodBody {
                item: "trait Listener::call".to_owned()
            }
            .to_string(),
            "`trait Listener::call` cannot define a default body"
        );
    }
}

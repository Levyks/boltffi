use boltffi_ast::{AttributeInput, Path, UserAttr};
use syn::parse::Parser;

use crate::ScanError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Marker {
    Data,
    DataImpl,
    Error,
    Export,
    Skip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    Skip,
    Reject(Marker),
    Unmarked,
}

pub fn disposition(attrs: &[syn::Attribute]) -> Result<Disposition, ScanError> {
    Ok(match Marker::detect(attrs)? {
        Some(Marker::Skip) => Disposition::Skip,
        Some(marker) => Disposition::Reject(marker),
        None => Disposition::Unmarked,
    })
}

impl Marker {
    pub fn detect(attrs: &[syn::Attribute]) -> Result<Option<Self>, ScanError> {
        attrs.iter().try_fold(None, |detected: Option<Self>, attr| {
            let marker = Self::from_attribute(attr)?;
            match (detected, marker) {
                (Some(first), Some(second)) => Err(ScanError::ConflictingMarkers {
                    first: first.as_str().to_owned(),
                    second: second.as_str().to_owned(),
                }),
                (None, Some(marker)) => Ok(Some(marker)),
                (detected, None) => Ok(detected),
            }
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::DataImpl => "data(impl)",
            Self::Error => "error",
            Self::Export => "export",
            Self::Skip => "skip",
        }
    }

    pub fn invalid_placement(self, item: impl Into<String>) -> ScanError {
        ScanError::InvalidMarkerPlacement {
            marker: self.as_str().to_owned(),
            item: item.into(),
        }
    }

    pub fn append_value_attrs(self, attrs: &mut Vec<UserAttr>) {
        if self == Self::Error {
            attrs.push(UserAttr::new(Path::single("error"), AttributeInput::Empty));
        }
    }

    fn from_attribute(attr: &syn::Attribute) -> Result<Option<Self>, ScanError> {
        match marker_name(attr).as_deref() {
            Some("data") => Self::from_data(attr).map(Some),
            Some("error") => Self::empty(attr, Self::Error).map(Some),
            Some("export") => Self::empty(attr, Self::Export).map(Some),
            Some("skip") => Self::empty(attr, Self::Skip).map(Some),
            _ => Ok(None),
        }
    }

    fn empty(attr: &syn::Attribute, marker: Self) -> Result<Self, ScanError> {
        match &attr.meta {
            syn::Meta::Path(_) => Ok(marker),
            _ => Err(invalid(attr)),
        }
    }

    fn from_data(attr: &syn::Attribute) -> Result<Self, ScanError> {
        match &attr.meta {
            syn::Meta::Path(_) => Ok(Self::Data),
            syn::Meta::List(list) => parse_data_impl
                .parse2(list.tokens.clone())
                .map(|_| Self::DataImpl)
                .map_err(|_| invalid(attr)),
            _ => Err(invalid(attr)),
        }
    }
}

fn parse_data_impl(input: syn::parse::ParseStream<'_>) -> syn::Result<()> {
    input.parse::<syn::Token![impl]>()?;
    Ok(())
}

fn marker_name(attr: &syn::Attribute) -> Option<String> {
    let segments = attr.path().segments.iter().collect::<Vec<_>>();
    match segments.as_slice() {
        [segment] => Some(segment.ident.to_string())
            .filter(|name| matches!(name.as_str(), "data" | "error" | "export" | "skip")),
        [namespace, marker] if namespace.ident == "boltffi" => Some(marker.ident.to_string())
            .filter(|name| matches!(name.as_str(), "data" | "error" | "export" | "skip")),
        _ => None,
    }
}

fn invalid(attr: &syn::Attribute) -> ScanError {
    ScanError::InvalidMarker {
        attribute: crate::spelling::attr(attr),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn struct_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemStruct>(source)
            .expect("valid struct")
            .attrs
    }

    fn impl_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemImpl>(source)
            .expect("valid impl")
            .attrs
    }

    fn fn_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemFn>(source)
            .expect("valid fn")
            .attrs
    }

    fn enum_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemEnum>(source)
            .expect("valid enum")
            .attrs
    }

    fn const_attrs(source: &str) -> Vec<syn::Attribute> {
        syn::parse_str::<syn::ItemConst>(source)
            .expect("valid const")
            .attrs
    }

    #[test]
    fn detects_data_on_value_types() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[data] struct S { x: i32 }")),
            Ok(Some(Marker::Data))
        );
        assert_eq!(
            Marker::detect(&struct_attrs("#[boltffi::data] struct S { x: i32 }")),
            Ok(Some(Marker::Data))
        );
        assert_eq!(
            Marker::detect(&struct_attrs("struct S { x: i32 }")),
            Ok(None)
        );
        assert_eq!(
            Marker::detect(&struct_attrs("#[derive(Clone)] struct S { x: i32 }")),
            Ok(None)
        );
    }

    #[test]
    fn detects_data_impl_distinctly_from_data() {
        assert_eq!(
            Marker::detect(&impl_attrs("#[data(impl)] impl S {}")),
            Ok(Some(Marker::DataImpl))
        );
        assert_eq!(
            Marker::detect(&impl_attrs("#[boltffi::data(impl)] impl S {}")),
            Ok(Some(Marker::DataImpl))
        );
        assert_eq!(Marker::detect(&impl_attrs("impl S {}")), Ok(None));
    }

    #[test]
    fn rejects_unknown_marker_arguments() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[data(foo)] struct S { x: i32 }")),
            Err(ScanError::InvalidMarker {
                attribute: "data(foo)".to_owned()
            })
        );
        assert_eq!(
            Marker::detect(&fn_attrs("#[export(foo)] fn f() {}")),
            Err(ScanError::InvalidMarker {
                attribute: "export(foo)".to_owned()
            })
        );
    }

    #[test]
    fn rejects_conflicting_markers() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[data] #[error] struct S { x: i32 }")),
            Err(ScanError::ConflictingMarkers {
                first: "data".to_owned(),
                second: "error".to_owned()
            })
        );
    }

    #[test]
    fn ignores_unowned_qualified_attributes() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[other::data] struct S { x: i32 }")),
            Ok(None)
        );
    }

    #[test]
    fn detects_error_on_value_types() {
        assert_eq!(
            Marker::detect(&struct_attrs("#[error] struct E { code: i32 }")),
            Ok(Some(Marker::Error))
        );
        assert_eq!(
            Marker::detect(&enum_attrs("#[boltffi::error] enum E { Io, Parse }")),
            Ok(Some(Marker::Error))
        );
    }

    #[test]
    fn detects_export_on_exported_items() {
        assert_eq!(
            Marker::detect(&fn_attrs("#[export] fn f() {}")),
            Ok(Some(Marker::Export))
        );
        assert_eq!(
            Marker::detect(&fn_attrs("#[boltffi::export] fn f() {}")),
            Ok(Some(Marker::Export))
        );
        assert_eq!(Marker::detect(&fn_attrs("fn f() {}")), Ok(None));
        assert_eq!(
            Marker::detect(&const_attrs("#[export] const ANSWER: u32 = 42;")),
            Ok(Some(Marker::Export))
        );
    }

    #[test]
    fn detects_skip_through_the_marker_set() {
        assert_eq!(
            Marker::detect(&fn_attrs("#[skip] fn f() {}")),
            Ok(Some(Marker::Skip))
        );
        assert_eq!(
            Marker::detect(&fn_attrs("#[boltffi::skip] fn f() {}")),
            Ok(Some(Marker::Skip))
        );
        assert_eq!(
            Marker::detect(&fn_attrs("#[skip(reason)] fn f() {}")),
            Err(ScanError::InvalidMarker {
                attribute: "skip(reason)".to_owned()
            })
        );
    }
}

use syn::punctuated::Punctuated;

use crate::ScanError;

pub fn generics(generics: &syn::Generics, item: &str) -> Result<(), ScanError> {
    if !generics.params.is_empty() || generics.where_clause.is_some() {
        return Err(ScanError::UnsupportedGenerics {
            item: item.to_owned(),
        });
    }
    Ok(())
}

pub fn unsafety(unsafety: Option<&syn::token::Unsafe>, item: &str) -> Result<(), ScanError> {
    if unsafety.is_some() {
        return Err(ScanError::UnsupportedUnsafe {
            item: item.to_owned(),
        });
    }
    Ok(())
}

pub fn extern_abi(abi: Option<&syn::Abi>, item: &str) -> Result<(), ScanError> {
    if abi.is_some() {
        return Err(ScanError::UnsupportedExternAbi {
            item: item.to_owned(),
        });
    }
    Ok(())
}

pub fn supertraits(
    bounds: &Punctuated<syn::TypeParamBound, syn::Token![+]>,
    item: &str,
) -> Result<(), ScanError> {
    if !bounds.is_empty() {
        return Err(ScanError::UnsupportedSupertraits {
            item: item.to_owned(),
        });
    }
    Ok(())
}

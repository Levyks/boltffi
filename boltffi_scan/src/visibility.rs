use boltffi_ast::{Source, Visibility};

pub(super) fn scan(visibility: &syn::Visibility) -> Source {
    Source::new(scan_visibility(visibility), None)
}

fn scan_visibility(visibility: &syn::Visibility) -> Visibility {
    match visibility {
        syn::Visibility::Public(_) => Visibility::Public,
        syn::Visibility::Restricted(restricted) => {
            Visibility::Restricted(path_spelling(&restricted.path))
        }
        syn::Visibility::Inherited => Visibility::Private,
    }
}

fn path_spelling(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visibility(source: &str) -> syn::Visibility {
        syn::parse_str(source).expect("valid visibility")
    }

    #[test]
    fn scans_public_visibility() {
        assert_eq!(scan_visibility(&visibility("pub")), Visibility::Public);
    }

    #[test]
    fn scans_private_visibility() {
        assert_eq!(
            scan_visibility(&syn::Visibility::Inherited),
            Visibility::Private
        );
    }

    #[test]
    fn scans_restricted_visibility() {
        assert_eq!(
            scan_visibility(&visibility("pub(crate)")),
            Visibility::Restricted("crate".to_owned())
        );
        assert_eq!(
            scan_visibility(&visibility("pub(super)")),
            Visibility::Restricted("super".to_owned())
        );
        assert_eq!(
            scan_visibility(&visibility("pub(in crate::ffi)")),
            Visibility::Restricted("crate::ffi".to_owned())
        );
    }

    #[test]
    fn source_never_invents_span_without_input_file_context() {
        assert_eq!(
            scan(&visibility("pub")),
            Source::new(Visibility::Public, None)
        );
    }
}

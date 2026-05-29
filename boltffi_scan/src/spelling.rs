pub(super) fn path(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

pub(super) fn ty(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Paren(paren) => self::ty(&paren.elem),
        syn::Type::Group(group) => self::ty(&group.elem),
        syn::Type::Path(type_path) => path(&type_path.path),
        syn::Type::Reference(reference) => format!("&{}", self::ty(&reference.elem)),
        _ => "unrecognized type".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    fn parse_path(source: &str) -> syn::Path {
        syn::parse_str(source).expect("valid path")
    }

    fn parse_type(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    #[test]
    fn joins_path_segments_with_double_colon() {
        assert_eq!(
            super::path(&parse_path("crate::geometry::Point")),
            "crate::geometry::Point"
        );
    }

    #[test]
    fn renders_paths_groups_and_references_for_types() {
        assert_eq!(super::ty(&parse_type("Point")), "Point");
        assert_eq!(super::ty(&parse_type("&Point")), "&Point");
        assert_eq!(super::ty(&parse_type("(Point)")), "Point");
    }

    #[test]
    fn renders_unrecognized_types_with_a_stable_label() {
        assert_eq!(super::ty(&parse_type("[u8; 4]")), "unrecognized type");
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ModulePath {
    segments: Vec<String>,
}

impl ModulePath {
    pub(super) fn root(crate_name: impl Into<String>) -> Self {
        Self {
            segments: vec![crate_name.into()],
        }
    }

    pub(super) fn child(&self, module: impl Into<String>) -> Self {
        let mut segments = self.segments.clone();
        segments.push(module.into());
        Self { segments }
    }

    pub(super) fn qualified(&self, ident: &str) -> String {
        let mut path = self.segments.join("::");
        path.push_str("::");
        path.push_str(ident);
        path
    }

    pub(super) fn resolve(&self, path: &syn::Path) -> Option<String> {
        if path.leading_colon.is_some() {
            return None;
        }

        let segments = path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>();
        let (first, rest) = segments.split_first()?;
        match first.as_str() {
            "crate" => {
                let mut resolved = self
                    .segments
                    .first()
                    .cloned()
                    .into_iter()
                    .collect::<Vec<_>>();
                resolved.extend(rest.iter().cloned());
                Some(resolved.join("::"))
            }
            "self" => {
                let mut resolved = self.segments.clone();
                resolved.extend(rest.iter().cloned());
                Some(resolved.join("::"))
            }
            "super" => {
                let super_count = segments
                    .iter()
                    .take_while(|segment| segment.as_str() == "super")
                    .count();
                let retained_segments = self.segments.len().saturating_sub(super_count).max(1);
                let mut resolved = self
                    .segments
                    .iter()
                    .take(retained_segments)
                    .cloned()
                    .collect::<Vec<_>>();
                resolved.extend(segments.iter().skip(super_count).cloned());
                Some(resolved.join("::"))
            }
            _ => {
                let mut resolved = self.segments.clone();
                resolved.extend(segments);
                Some(resolved.join("::"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_qualifies_items_under_the_crate_segment() {
        assert_eq!(ModulePath::root("demo").qualified("add"), "demo::add");
    }

    #[test]
    fn child_paths_preserve_all_ancestors_in_order() {
        let path = ModulePath::root("demo").child("geometry").child("point");

        assert_eq!(path.qualified("Point"), "demo::geometry::point::Point");
    }

    #[test]
    fn child_does_not_mutate_the_parent_path() {
        let parent = ModulePath::root("demo");
        let child = parent.child("geometry");

        assert_eq!(parent.qualified("Point"), "demo::Point");
        assert_eq!(child.qualified("Point"), "demo::geometry::Point");
    }

    #[test]
    fn resolves_type_paths_from_module_context() {
        let module = ModulePath::root("demo").child("geometry").child("shape");

        assert_eq!(
            module.resolve(&syn::parse_str("Point").expect("path")),
            Some("demo::geometry::shape::Point".to_owned())
        );
        assert_eq!(
            module.resolve(&syn::parse_str("self::Point").expect("path")),
            Some("demo::geometry::shape::Point".to_owned())
        );
        assert_eq!(
            module.resolve(&syn::parse_str("super::Point").expect("path")),
            Some("demo::geometry::Point".to_owned())
        );
        assert_eq!(
            module.resolve(&syn::parse_str("crate::Point").expect("path")),
            Some("demo::Point".to_owned())
        );
    }
}

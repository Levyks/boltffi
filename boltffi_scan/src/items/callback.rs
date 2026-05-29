use boltffi_ast::{MethodDef, TraitDef, TraitId};

use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::marker::Marker;
use crate::type_expr::Scanner;
use crate::{ModulePath, ScanError, name, visibility};

use super::signature;

pub fn scan(
    marked: &Marked<'_, syn::ItemTrait>,
    declared_types: &DeclaredTypes,
) -> Result<TraitDef, ScanError> {
    build(marked.item(), marked.module(), declared_types)
}

fn build(
    item: &syn::ItemTrait,
    module: &ModulePath,
    declared_types: &DeclaredTypes,
) -> Result<TraitDef, ScanError> {
    let item_name = format!("trait {}", item.ident);
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(ScanError::UnsupportedGenerics { item: item_name });
    }
    if item.unsafety.is_some() {
        return Err(ScanError::UnsupportedUnsafe { item: item_name });
    }
    if !item.supertraits.is_empty() {
        return Err(ScanError::UnsupportedSupertraits { item: item_name });
    }

    let id = TraitId::new(module.qualified(&item.ident.to_string()));
    let mut callback = TraitDef::new(id, name::canonical(&item.ident));
    callback.source = visibility::scan(&item.vis);
    callback.methods = methods(
        item,
        callback.id.as_str(),
        &Scanner::new(declared_types, module),
    )?;
    Ok(callback)
}

fn methods(
    item: &syn::ItemTrait,
    parent: &str,
    scanner: &Scanner<'_>,
) -> Result<Vec<MethodDef>, ScanError> {
    let trait_name = item.ident.to_string();
    item.items
        .iter()
        .filter_map(|trait_item| match trait_item {
            syn::TraitItem::Fn(method) => match is_exported_method(method) {
                Ok(true) => Some(method_from_signature(method, parent, &trait_name, scanner)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            },
            other => Some(Err(unsupported_trait_item(other, &trait_name))),
        })
        .collect()
}

fn is_exported_method(method: &syn::TraitItemFn) -> Result<bool, ScanError> {
    match Marker::detect(&method.attrs)? {
        Some(Marker::Skip) => Ok(false),
        Some(marker) => Err(ScanError::InvalidMarkerPlacement {
            marker: marker.as_str().to_owned(),
            item: "trait method".to_owned(),
        }),
        None => Ok(true),
    }
}

fn method_from_signature(
    method: &syn::TraitItemFn,
    parent: &str,
    trait_name: &str,
    scanner: &Scanner<'_>,
) -> Result<MethodDef, ScanError> {
    if method.default.is_some() {
        return Err(ScanError::UnsupportedTraitMethodBody {
            item: format!("trait {trait_name}::{}", method.sig.ident),
        });
    }
    signature::method(&method.sig, parent, scanner)
}

fn unsupported_trait_item(item: &syn::TraitItem, parent: &str) -> ScanError {
    ScanError::UnsupportedTraitItem {
        item: format!("trait {parent}::{}", trait_item_name(item)),
    }
}

fn trait_item_name(item: &syn::TraitItem) -> String {
    match item {
        syn::TraitItem::Const(item) => item.ident.to_string(),
        syn::TraitItem::Type(item) => item.ident.to_string(),
        syn::TraitItem::Macro(item) => item
            .mac
            .path
            .segments
            .last()
            .map_or_else(|| "macro".to_owned(), |segment| segment.ident.to_string()),
        _ => "item".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        CanonicalName, ExecutionKind, NamePart, Primitive, Receiver, ReturnDef, Source, TraitId,
        TypeExpr, Visibility,
    };

    fn parse(source: &str) -> syn::ItemTrait {
        syn::parse_str(source).expect("valid trait source")
    }

    fn scan(source: &str) -> Result<TraitDef, ScanError> {
        build(
            &parse(source),
            &ModulePath::root("demo"),
            &DeclaredTypes::new(),
        )
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn scans_complete_callback_trait_contract() {
        let callback = scan("pub trait ValueCallback { fn on_value(&self, value: i32) -> i64; }")
            .expect("scan");

        assert_eq!(callback.id, TraitId::new("demo::ValueCallback"));
        assert_eq!(callback.name, name(&["value", "callback"]));
        assert_eq!(callback.source, Source::new(Visibility::Public, None));
        assert_eq!(callback.methods.len(), 1);
        assert_eq!(
            callback.methods[0].id,
            "demo::ValueCallback::on_value".into()
        );
        assert_eq!(callback.methods[0].name, name(&["on", "value"]));
        assert_eq!(callback.methods[0].receiver, Receiver::Shared);
        assert_eq!(callback.methods[0].execution, ExecutionKind::Sync);
        assert_eq!(
            callback.methods[0].parameters[0].type_expr,
            TypeExpr::Primitive(Primitive::I32)
        );
        assert_eq!(
            callback.methods[0].returns,
            ReturnDef::Value(TypeExpr::Primitive(Primitive::I64))
        );
    }

    #[test]
    fn scans_async_callback_method_execution() {
        let callback =
            scan("trait Loader { async fn load(&self, key: u32) -> u64; }").expect("scan");

        assert_eq!(callback.methods[0].execution, ExecutionKind::Async);
    }

    #[test]
    fn skips_explicitly_skipped_callback_methods() {
        let callback =
            scan("trait Listener { #[skip] fn local(&self); fn remote(&self); }").expect("scan");

        assert_eq!(callback.methods.len(), 1);
        assert_eq!(callback.methods[0].name, name(&["remote"]));
    }

    #[test]
    fn rejects_trait_shapes_erased_by_the_ast_contract() {
        let generic =
            scan("trait Listener<T> { fn call(&self, value: T); }").expect_err("generic rejected");
        let unsafe_trait =
            scan("unsafe trait Listener { fn call(&self); }").expect_err("unsafe rejected");
        let supertrait =
            scan("trait Listener: Send { fn call(&self); }").expect_err("supertrait rejected");

        assert_eq!(
            generic,
            ScanError::UnsupportedGenerics {
                item: "trait Listener".to_owned()
            }
        );
        assert_eq!(
            unsafe_trait,
            ScanError::UnsupportedUnsafe {
                item: "trait Listener".to_owned()
            }
        );
        assert_eq!(
            supertrait,
            ScanError::UnsupportedSupertraits {
                item: "trait Listener".to_owned()
            }
        );
    }

    #[test]
    fn rejects_unrepresentable_trait_items() {
        let associated_type =
            scan("trait Listener { type Value; fn call(&self); }").expect_err("type rejected");
        let associated_const = scan("trait Listener { const LIMIT: usize; fn call(&self); }")
            .expect_err("const rejected");
        let default_body = scan("trait Listener { fn call(&self) {} }").expect_err("body rejected");

        assert_eq!(
            associated_type,
            ScanError::UnsupportedTraitItem {
                item: "trait Listener::Value".to_owned()
            }
        );
        assert_eq!(
            associated_const,
            ScanError::UnsupportedTraitItem {
                item: "trait Listener::LIMIT".to_owned()
            }
        );
        assert_eq!(
            default_body,
            ScanError::UnsupportedTraitMethodBody {
                item: "trait Listener::call".to_owned()
            }
        );
    }

    #[test]
    fn rejects_non_skip_boltffi_markers_on_trait_methods() {
        let error = scan("trait Listener { #[export] fn local(&self); }")
            .expect_err("misplaced marker must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "trait method".to_owned()
            }
        );
    }
}

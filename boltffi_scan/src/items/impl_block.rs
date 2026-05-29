use boltffi_ast::{EnumDef, MethodDef, MethodId, ParameterDef, Receiver, RecordDef};

use crate::declared_types::{DeclaredType, DeclaredTypes};
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::{ModulePath, ScanError, name};

use super::signature;

pub fn attach_methods(
    impls: &[Marked<'_, syn::ItemImpl>],
    declared_types: &DeclaredTypes,
    records: &mut [RecordDef],
    enums: &mut [EnumDef],
) -> Result<(), ScanError> {
    impls
        .iter()
        .try_for_each(|item| attach_impl(item, declared_types, records, enums))
}

fn attach_impl(
    item: &Marked<'_, syn::ItemImpl>,
    declared_types: &DeclaredTypes,
    records: &mut [RecordDef],
    enums: &mut [EnumDef],
) -> Result<(), ScanError> {
    if !item.item().generics.params.is_empty() || item.item().generics.where_clause.is_some() {
        return Err(ScanError::UnsupportedGenerics {
            item: format!("impl {}", target_spelling(item.item())),
        });
    }
    let target = resolve_impl_target(item.item(), item.module(), declared_types)?;
    let scanned = scan_methods(item.item(), target.id(), item.module(), declared_types)?;
    match target {
        ImplTarget::Record(id) => {
            if let Some(record) = records.iter_mut().find(|record| record.id == id) {
                record.methods.extend(scanned);
            }
        }
        ImplTarget::Enum(id) => {
            if let Some(enumeration) = enums.iter_mut().find(|enumeration| enumeration.id == id) {
                enumeration.methods.extend(scanned);
            }
        }
    }
    Ok(())
}

fn scan_methods(
    item: &syn::ItemImpl,
    parent: &str,
    module: &ModulePath,
    declared_types: &DeclaredTypes,
) -> Result<Vec<MethodDef>, ScanError> {
    let scanner = Scanner::new(declared_types, module);
    item.items
        .iter()
        .filter_map(|impl_item| match impl_item {
            syn::ImplItem::Fn(method) if is_exported_method(method) => {
                Some(scan_method(method, parent, &scanner))
            }
            _ => None,
        })
        .collect()
}

fn is_exported_method(method: &syn::ImplItemFn) -> bool {
    matches!(method.vis, syn::Visibility::Public(_))
        && !method.attrs.iter().any(|attribute| {
            attribute
                .path()
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "skip")
        })
}

fn scan_method(
    method: &syn::ImplItemFn,
    parent: &str,
    scanner: &Scanner<'_>,
) -> Result<MethodDef, ScanError> {
    let ident = &method.sig.ident;
    signature::validate(&method.sig, format!("method {parent}::{ident}"))?;
    let mut declaration = MethodDef::new(
        MethodId::new(format!("{parent}::{ident}")),
        name::canonical(ident),
        receiver(&method.sig),
    );
    declaration.execution = signature::execution(&method.sig);
    declaration.parameters = parameters(&method.sig, scanner)?;
    declaration.returns = scanner.scan_return(&method.sig.output)?;
    Ok(declaration)
}

fn receiver(signature: &syn::Signature) -> Receiver {
    match signature.inputs.first() {
        Some(syn::FnArg::Receiver(receiver)) => {
            match (receiver.reference.is_some(), receiver.mutability.is_some()) {
                (true, true) => Receiver::Mutable,
                (true, false) => Receiver::Shared,
                (false, _) => Receiver::Owned,
            }
        }
        _ => Receiver::None,
    }
}

fn parameters(
    signature: &syn::Signature,
    scanner: &Scanner<'_>,
) -> Result<Vec<ParameterDef>, ScanError> {
    signature
        .inputs
        .iter()
        .filter_map(|argument| match argument {
            syn::FnArg::Typed(typed) => Some(signature::parameter(typed, scanner)),
            syn::FnArg::Receiver(_) => None,
        })
        .collect()
}

fn self_type_path(item: &syn::ItemImpl) -> Option<&syn::Path> {
    let syn::Type::Path(type_path) = item.self_ty.as_ref() else {
        return None;
    };
    Some(&type_path.path)
}

fn target_spelling(item: &syn::ItemImpl) -> String {
    match item.self_ty.as_ref() {
        syn::Type::Path(type_path) => type_path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        _ => "unrecognized impl target".to_owned(),
    }
}

enum ImplTarget {
    Record(boltffi_ast::RecordId),
    Enum(boltffi_ast::EnumId),
}

impl ImplTarget {
    fn id(&self) -> &str {
        match self {
            Self::Record(id) => id.as_str(),
            Self::Enum(id) => id.as_str(),
        }
    }
}

fn resolve_impl_target(
    item: &syn::ItemImpl,
    module: &ModulePath,
    declared_types: &DeclaredTypes,
) -> Result<ImplTarget, ScanError> {
    let Some(target) = self_type_path(item).and_then(|path| module.resolve(path)) else {
        return Err(ScanError::UnsupportedMarkedImpl {
            target: target_spelling(item),
        });
    };
    match declared_types.resolve(&target) {
        Some(DeclaredType::Record(id)) => Ok(ImplTarget::Record(id.clone())),
        Some(DeclaredType::Enum(id)) => Ok(ImplTarget::Enum(id.clone())),
        None => Err(ScanError::UnsupportedMarkedImpl {
            target: target_spelling(item),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::declared_types::DeclaredTypes;
    use boltffi_ast::{CanonicalName, EnumId, NamePart, Primitive, RecordId, ReturnDef, TypeExpr};

    fn parse(source: &str) -> syn::ItemImpl {
        syn::parse_str(source).expect("valid impl block")
    }

    fn declared_point() -> DeclaredTypes {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        declared_types
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    fn scan(
        source: &str,
        parent: &str,
        declared_types: &DeclaredTypes,
    ) -> Result<Vec<MethodDef>, ScanError> {
        scan_methods(
            &parse(source),
            parent,
            &ModulePath::root("demo"),
            declared_types,
        )
    }

    #[test]
    fn scans_borrowing_method_with_resolved_param_and_return() {
        let declared_types = declared_point();
        let methods = scan(
            "impl Point { pub fn distance(&self, other: Point) -> f64 { 0.0 } }",
            "demo::Point",
            &declared_types,
        )
        .expect("scan");

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].id, MethodId::new("demo::Point::distance"));
        assert_eq!(methods[0].name, name(&["distance"]));
        assert_eq!(methods[0].receiver, Receiver::Shared);
        assert_eq!(
            methods[0].parameters[0].type_expr,
            TypeExpr::Record(RecordId::new("demo::Point"))
        );
        assert_eq!(
            methods[0].returns,
            ReturnDef::Value(TypeExpr::Primitive(Primitive::F64))
        );
    }

    #[test]
    fn associated_function_returning_self_has_no_receiver() {
        let methods = scan(
            "impl Point { pub fn origin() -> Self { todo!() } }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods[0].receiver, Receiver::None);
        assert_eq!(methods[0].returns, ReturnDef::Value(TypeExpr::SelfType));
    }

    #[test]
    fn captures_each_receiver_shape() {
        let methods = scan(
            "impl Point { \
                pub fn shared(&self) {} \
                pub fn exclusive(&mut self) {} \
                pub fn consuming(self) {} \
            }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods[0].receiver, Receiver::Shared);
        assert_eq!(methods[1].receiver, Receiver::Mutable);
        assert_eq!(methods[2].receiver, Receiver::Owned);
    }

    #[test]
    fn skips_private_and_explicitly_skipped_methods() {
        let methods = scan(
            "impl Point { \
                pub fn exported(&self) {} \
                fn helper(&self) {} \
                #[skip] pub fn skipped(&self) {} \
            }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect("scan");

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].name, name(&["exported"]));
    }

    #[test]
    fn resolves_enum_impl_targets() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_enum(EnumId::new("demo::Mode"));

        let target = resolve_impl_target(
            &parse("impl Mode { pub fn parse() -> Self { todo!() } }"),
            &ModulePath::root("demo"),
            &declared_types,
        )
        .expect("target");

        assert!(matches!(target, ImplTarget::Enum(id) if id == EnumId::new("demo::Mode")));
    }

    #[test]
    fn rejects_generic_methods_before_erasing_type_parameters() {
        let error = scan(
            "impl Point { pub fn value<T>(&self) -> i32 { 0 } }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("generic rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "method demo::Point::value".to_owned()
            }
        );
    }

    #[test]
    fn rejects_unsafe_methods_before_erasing_unsafety() {
        let error = scan(
            "impl Point { pub unsafe fn free(&self) {} }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("unsafe rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedUnsafe {
                item: "method demo::Point::free".to_owned()
            }
        );
    }

    #[test]
    fn rejects_extern_methods_before_erasing_abi() {
        let error = scan(
            "impl Point { pub extern \"C\" fn add(&self, value: i32) -> i32 { value } }",
            "demo::Point",
            &DeclaredTypes::new(),
        )
        .expect_err("extern rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedExternAbi {
                item: "method demo::Point::add".to_owned()
            }
        );
    }

    #[test]
    fn rejects_generic_impl_before_erasing_type_parameters() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let source_tree = crate::source_tree::SourceTree::in_memory(
            "demo",
            syn::parse_str::<syn::File>("#[data(impl)] impl<T> Point { pub fn get(&self) {} }")
                .expect("valid source")
                .items,
        )
        .expect("source tree");
        let marked = crate::marked::MarkedItems::collect(&source_tree).expect("marked");
        let mut records = vec![RecordDef::new(
            RecordId::new("demo::Point"),
            name(&["point"]),
        )];
        let mut enums = Vec::new();

        let error = attach_methods(marked.impls(), &declared_types, &mut records, &mut enums)
            .expect_err("generic rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "impl Point".to_owned()
            }
        );
    }
}

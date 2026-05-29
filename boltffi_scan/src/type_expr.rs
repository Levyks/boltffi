use boltffi_ast::{ClosureKind, ClosureType, Primitive, ReturnDef, TypeExpr};

use crate::declared_types::{DeclaredType, DeclaredTypes};
use crate::{ModulePath, ScanError};

pub(super) struct Scanner<'a> {
    declared_types: &'a DeclaredTypes,
    module: &'a ModulePath,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(declared_types: &'a DeclaredTypes, module: &'a ModulePath) -> Self {
        Self {
            declared_types,
            module,
        }
    }

    pub(super) fn scan(&self, ty: &syn::Type) -> Result<TypeExpr, ScanError> {
        match ty {
            syn::Type::ImplTrait(impl_trait) => self.closure(impl_trait, ty),
            syn::Type::Tuple(tuple) => self.tuple(tuple),
            syn::Type::Path(type_path) => self.path(type_path, ty),
            _ => Err(ScanError::unsupported_type(ty)),
        }
    }

    pub(super) fn scan_return(&self, output: &syn::ReturnType) -> Result<ReturnDef, ScanError> {
        match output {
            syn::ReturnType::Default => Ok(ReturnDef::Void),
            syn::ReturnType::Type(_, ty) if is_unit(ty) => Ok(ReturnDef::Void),
            syn::ReturnType::Type(_, ty) => Ok(ReturnDef::Value(self.scan(ty)?)),
        }
    }

    fn path(&self, type_path: &syn::TypePath, source: &syn::Type) -> Result<TypeExpr, ScanError> {
        if type_path.qself.is_some() {
            return Err(ScanError::unsupported_type(source));
        }
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| ScanError::unsupported_type(source))?;
        match segment.ident.to_string().as_str() {
            "Self" => Ok(TypeExpr::SelfType),
            "String" => Ok(TypeExpr::String),
            "Vec" => Ok(TypeExpr::vec(self.single_argument(segment, source)?)),
            "Option" => Ok(TypeExpr::option(self.single_argument(segment, source)?)),
            "Result" => {
                let (ok, err) = self.two_arguments(segment, source)?;
                Ok(TypeExpr::result(ok, err))
            }
            "HashMap" | "BTreeMap" => {
                let (key, value) = self.two_arguments(segment, source)?;
                Ok(TypeExpr::map(key, value))
            }
            _ => self.named(type_path, source),
        }
    }

    fn named(&self, type_path: &syn::TypePath, source: &syn::Type) -> Result<TypeExpr, ScanError> {
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| ScanError::unsupported_type(source))?;
        let name = segment.ident.to_string();
        if let Some(primitive) = Primitive::from_rust_name(&name) {
            return Ok(TypeExpr::Primitive(primitive));
        }
        let Some(resolved_path) = self.module.resolve(&type_path.path) else {
            return Err(ScanError::unsupported_type(source));
        };
        match self.declared_types.resolve(&resolved_path) {
            Some(DeclaredType::Record(id)) => Ok(TypeExpr::Record(id.clone())),
            Some(DeclaredType::Enum(id)) => Ok(TypeExpr::Enum(id.clone())),
            None => Err(ScanError::unsupported_type(source)),
        }
    }

    fn tuple(&self, tuple: &syn::TypeTuple) -> Result<TypeExpr, ScanError> {
        if tuple.elems.is_empty() {
            return Ok(TypeExpr::Unit);
        }
        let elements = tuple
            .elems
            .iter()
            .map(|element| self.scan(element))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TypeExpr::tuple(elements))
    }

    fn single_argument(
        &self,
        segment: &syn::PathSegment,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        match type_arguments(segment).as_slice() {
            [argument] => self.scan(argument),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn two_arguments(
        &self,
        segment: &syn::PathSegment,
        source: &syn::Type,
    ) -> Result<(TypeExpr, TypeExpr), ScanError> {
        match type_arguments(segment).as_slice() {
            [first, second] => Ok((self.scan(first)?, self.scan(second)?)),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn closure(
        &self,
        impl_trait: &syn::TypeImplTrait,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        let (kind, arguments) = impl_trait
            .bounds
            .iter()
            .find_map(|bound| match bound {
                syn::TypeParamBound::Trait(trait_bound) => closure_bound(trait_bound),
                _ => None,
            })
            .ok_or_else(|| ScanError::unsupported_type(source))?;
        let parameters = arguments
            .inputs
            .iter()
            .map(|input| self.scan(input))
            .collect::<Result<Vec<_>, _>>()?;
        let returns = self.scan_return(&arguments.output)?;
        Ok(TypeExpr::closure(ClosureType::new(
            kind, parameters, returns,
        )))
    }
}

fn is_unit(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn type_arguments(segment: &syn::PathSegment) -> Vec<&syn::Type> {
    match &segment.arguments {
        syn::PathArguments::AngleBracketed(bracketed) => bracketed
            .args
            .iter()
            .filter_map(|argument| match argument {
                syn::GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn closure_bound(
    bound: &syn::TraitBound,
) -> Option<(ClosureKind, &syn::ParenthesizedGenericArguments)> {
    let segment = bound.path.segments.last()?;
    let kind = closure_kind(&segment.ident.to_string())?;
    let syn::PathArguments::Parenthesized(arguments) = &segment.arguments else {
        return None;
    };
    Some((kind, arguments))
}

fn closure_kind(name: &str) -> Option<ClosureKind> {
    Some(match name {
        "Fn" => ClosureKind::Fn,
        "FnMut" => ClosureKind::FnMut,
        "FnOnce" => ClosureKind::FnOnce,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{EnumId, RecordId};

    fn ty(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    fn scan(source: &str) -> Result<TypeExpr, ScanError> {
        Scanner::new(&DeclaredTypes::new(), &ModulePath::root("demo")).scan(&ty(source))
    }

    #[test]
    fn scans_every_primitive_type_exactly() {
        [
            ("bool", Primitive::Bool),
            ("i8", Primitive::I8),
            ("u8", Primitive::U8),
            ("i16", Primitive::I16),
            ("u16", Primitive::U16),
            ("i32", Primitive::I32),
            ("u32", Primitive::U32),
            ("i64", Primitive::I64),
            ("u64", Primitive::U64),
            ("isize", Primitive::ISize),
            ("usize", Primitive::USize),
            ("f32", Primitive::F32),
            ("f64", Primitive::F64),
        ]
        .into_iter()
        .for_each(|(source, primitive)| {
            assert_eq!(scan(source), Ok(TypeExpr::Primitive(primitive)));
        });
    }

    #[test]
    fn scans_string_and_sequence_containers() {
        assert_eq!(scan("String"), Ok(TypeExpr::String));
        assert_eq!(
            scan("Vec<i32>"),
            Ok(TypeExpr::vec(TypeExpr::Primitive(Primitive::I32)))
        );
        assert_eq!(
            scan("Option<String>"),
            Ok(TypeExpr::option(TypeExpr::String))
        );
    }

    #[test]
    fn scans_result_and_map_containers() {
        assert_eq!(
            scan("Result<i32, String>"),
            Ok(TypeExpr::result(
                TypeExpr::Primitive(Primitive::I32),
                TypeExpr::String
            ))
        );
        assert_eq!(
            scan("HashMap<String, i32>"),
            Ok(TypeExpr::map(
                TypeExpr::String,
                TypeExpr::Primitive(Primitive::I32)
            ))
        );
    }

    #[test]
    fn scans_tuples_and_unit() {
        assert_eq!(
            scan("(i32, String)"),
            Ok(TypeExpr::tuple(vec![
                TypeExpr::Primitive(Primitive::I32),
                TypeExpr::String
            ]))
        );
        assert_eq!(scan("()"), Ok(TypeExpr::Unit));
    }

    #[test]
    fn scans_nested_containers() {
        assert_eq!(
            scan("Option<Vec<i32>>"),
            Ok(TypeExpr::option(TypeExpr::vec(TypeExpr::Primitive(
                Primitive::I32
            ))))
        );
    }

    #[test]
    fn resolves_qualified_std_paths_by_last_segment() {
        assert_eq!(scan("std::string::String"), Ok(TypeExpr::String));
        assert_eq!(
            scan("std::vec::Vec<u8>"),
            Ok(TypeExpr::vec(TypeExpr::Primitive(Primitive::U8)))
        );
    }

    #[test]
    fn resolves_registered_record_reference_including_nested() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::geometry::Point"));
        let module = ModulePath::root("demo").child("geometry");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Point")),
            Ok(TypeExpr::Record(RecordId::new("demo::geometry::Point")))
        );
        assert_eq!(
            scanner.scan(&ty("Vec<Point>")),
            Ok(TypeExpr::vec(TypeExpr::Record(RecordId::new(
                "demo::geometry::Point"
            ))))
        );
    }

    #[test]
    fn resolves_qualified_records_without_leaf_name_guessing() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::geometry::Point"));
        declared_types.register_record(RecordId::new("demo::Point"));
        declared_types.register_enum(EnumId::new("demo::geometry::Mode"));
        let module = ModulePath::root("demo").child("shape");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("crate::geometry::Point")),
            Ok(TypeExpr::Record(RecordId::new("demo::geometry::Point")))
        );
        assert_eq!(
            scanner.scan(&ty("crate::geometry::Mode")),
            Ok(TypeExpr::Enum(EnumId::new("demo::geometry::Mode")))
        );
        assert!(matches!(
            scanner.scan(&ty("Point")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Point"
        ));
    }

    #[test]
    fn unregistered_named_type_rejects_with_spelling() {
        assert!(matches!(
            scan("Point"),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Point"
        ));
    }

    #[test]
    fn self_type_is_captured_verbatim() {
        assert_eq!(scan("Self"), Ok(TypeExpr::SelfType));
    }

    #[test]
    fn impl_trait_closure_can_follow_marker_bounds() {
        let TypeExpr::Closure {
            signature,
            presence,
        } = scan("impl Send + Fn(u32) -> u32").expect("scan")
        else {
            panic!("expected closure");
        };

        assert_eq!(presence, boltffi_ast::HandlePresence::Required);
        assert_eq!(signature.kind, ClosureKind::Fn);
        assert_eq!(
            signature.parameters,
            vec![TypeExpr::Primitive(Primitive::U32)]
        );
        assert_eq!(
            signature.returns,
            ReturnDef::Value(TypeExpr::Primitive(Primitive::U32))
        );
    }

    #[test]
    fn impl_trait_without_fn_bound_is_rejected() {
        assert!(matches!(
            scan("impl Iterator<Item = u32>"),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "unrecognized type"
        ));
    }

    #[test]
    fn closure_with_unsupported_argument_reports_that_argument() {
        assert!(matches!(
            scan("impl Fn(Point)"),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Point"
        ));
    }
}

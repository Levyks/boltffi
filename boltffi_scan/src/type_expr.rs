use boltffi_ast::{
    ClosureKind, ClosureType, HandlePresence, Primitive, ReturnDef, TraitId, TraitUseForm, TypeExpr,
};

use crate::declared_types::{DeclaredType, DeclaredTypes};
use crate::{ModuleScope, ScanError};

pub(super) struct Scanner<'a> {
    declared_types: &'a DeclaredTypes,
    scope: &'a ModuleScope,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(declared_types: &'a DeclaredTypes, scope: &'a ModuleScope) -> Self {
        Self {
            declared_types,
            scope,
        }
    }

    pub(super) fn scan(&self, ty: &syn::Type) -> Result<TypeExpr, ScanError> {
        let unwrapped = unwrapped(ty);
        if let syn::Type::Path(type_path) = unwrapped
            && let Some(named) = self.exact_named(type_path)?
        {
            return Ok(named);
        }
        if let Some(custom) = self
            .declared_types
            .resolve_custom_remote(self.scope, unwrapped)?
        {
            return Ok(TypeExpr::Custom(custom.clone()));
        }
        match unwrapped {
            syn::Type::ImplTrait(impl_trait) => self.impl_trait(impl_trait, ty),
            syn::Type::Tuple(tuple) => self.tuple(tuple),
            syn::Type::Path(type_path) => self.path(type_path, ty),
            _ => Err(ScanError::unsupported_type(ty)),
        }
    }

    fn exact_named(&self, type_path: &syn::TypePath) -> Result<Option<TypeExpr>, ScanError> {
        if type_path.qself.is_some()
            || type_path
                .path
                .segments
                .iter()
                .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Ok(None);
        }
        let Some(segment) = type_path.path.segments.last() else {
            return Ok(None);
        };
        let name = segment.ident.to_string();
        if let Some(primitive) = Primitive::from_rust_name(&name) {
            return Ok(Some(TypeExpr::Primitive(primitive)));
        }
        match self
            .declared_types
            .resolve_in_scope(self.scope, &type_path.path)?
        {
            Some(DeclaredType::Record(id)) => Ok(Some(TypeExpr::Record(id.clone()))),
            Some(DeclaredType::Enum(id)) => Ok(Some(TypeExpr::Enum(id.clone()))),
            Some(DeclaredType::Class(id)) => {
                Ok(Some(TypeExpr::class(id.clone(), HandlePresence::Required)))
            }
            Some(DeclaredType::Custom(_) | DeclaredType::Trait(_)) | None => Ok(None),
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
            "Option" => self.option(segment, source),
            "Result" => {
                let (ok, err) = self.two_arguments(segment, source)?;
                Ok(TypeExpr::result(ok, err))
            }
            "HashMap" | "BTreeMap" => {
                let (key, value) = self.two_arguments(segment, source)?;
                Ok(TypeExpr::map(key, value))
            }
            "Box" => self.trait_object_argument(
                segment,
                source,
                TraitUseForm::BoxedDyn,
                HandlePresence::Required,
            ),
            "Arc" => self.trait_object_argument(
                segment,
                source,
                TraitUseForm::ArcDyn,
                HandlePresence::Required,
            ),
            _ => self.named(type_path, source),
        }
    }

    fn named(&self, type_path: &syn::TypePath, source: &syn::Type) -> Result<TypeExpr, ScanError> {
        let segment = type_path
            .path
            .segments
            .last()
            .ok_or_else(|| ScanError::unsupported_type(source))?;
        if type_path
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(ScanError::unsupported_type(source));
        }
        let name = segment.ident.to_string();
        if let Some(primitive) = Primitive::from_rust_name(&name) {
            return Ok(TypeExpr::Primitive(primitive));
        }
        match self
            .declared_types
            .resolve_in_scope(self.scope, &type_path.path)?
        {
            Some(DeclaredType::Record(id)) => Ok(TypeExpr::Record(id.clone())),
            Some(DeclaredType::Enum(id)) => Ok(TypeExpr::Enum(id.clone())),
            Some(DeclaredType::Class(id)) => {
                Ok(TypeExpr::class(id.clone(), HandlePresence::Required))
            }
            Some(DeclaredType::Custom(_)) => Err(ScanError::unsupported_type(source)),
            Some(DeclaredType::Trait(_)) => Err(ScanError::unsupported_type(source)),
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
        self.single_type_argument(segment, source)
            .and_then(|argument| self.scan(argument))
    }

    fn single_type_argument<'segment>(
        &self,
        segment: &'segment syn::PathSegment,
        source: &syn::Type,
    ) -> Result<&'segment syn::Type, ScanError> {
        match type_arguments(segment).as_slice() {
            [argument] => Ok(argument),
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

    fn option(
        &self,
        segment: &syn::PathSegment,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        let inner = self.single_type_argument(segment, source)?;
        self.nullable_handle(inner)
            .unwrap_or_else(|| self.scan(inner).map(TypeExpr::option))
    }

    fn nullable_handle(&self, ty: &syn::Type) -> Option<Result<TypeExpr, ScanError>> {
        let syn::Type::Path(type_path) = unwrapped(ty) else {
            return None;
        };
        let segment = type_path.path.segments.last()?;
        match segment.ident.to_string().as_str() {
            "Box" => Some(self.trait_object_argument(
                segment,
                ty,
                TraitUseForm::BoxedDyn,
                HandlePresence::Nullable,
            )),
            "Arc" => Some(self.trait_object_argument(
                segment,
                ty,
                TraitUseForm::ArcDyn,
                HandlePresence::Nullable,
            )),
            _ => self.nullable_class(type_path),
        }
    }

    fn nullable_class(&self, type_path: &syn::TypePath) -> Option<Result<TypeExpr, ScanError>> {
        if type_path
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return None;
        }
        match self
            .declared_types
            .resolve_in_scope(self.scope, &type_path.path)
        {
            Ok(Some(DeclaredType::Class(id))) => {
                Some(Ok(TypeExpr::class(id.clone(), HandlePresence::Nullable)))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        }
    }

    fn declared_trait(&self, path: &syn::Path, source: &syn::Type) -> Result<TraitId, ScanError> {
        match self.declared_types.resolve_in_scope(self.scope, path)? {
            Some(DeclaredType::Trait(id)) => Ok(id.clone()),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn impl_trait(
        &self,
        impl_trait: &syn::TypeImplTrait,
        source: &syn::Type,
    ) -> Result<TypeExpr, ScanError> {
        if let Some((kind, arguments)) = impl_trait.bounds.iter().find_map(|bound| match bound {
            syn::TypeParamBound::Trait(trait_bound) => closure_bound(trait_bound),
            _ => None,
        }) {
            return self.closure(kind, arguments);
        }

        match impl_trait.bounds.iter().collect::<Vec<_>>().as_slice() {
            [syn::TypeParamBound::Trait(bound)] => self.trait_bound(
                bound,
                source,
                TraitUseForm::ImplTrait,
                HandlePresence::Required,
            ),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn closure(
        &self,
        kind: ClosureKind,
        arguments: &syn::ParenthesizedGenericArguments,
    ) -> Result<TypeExpr, ScanError> {
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

    fn trait_object_argument(
        &self,
        segment: &syn::PathSegment,
        source: &syn::Type,
        form: TraitUseForm,
        presence: HandlePresence,
    ) -> Result<TypeExpr, ScanError> {
        let argument = self.single_type_argument(segment, source)?;
        let syn::Type::TraitObject(trait_object) = unwrapped(argument) else {
            return Err(ScanError::unsupported_type(source));
        };
        match trait_object.bounds.iter().collect::<Vec<_>>().as_slice() {
            [syn::TypeParamBound::Trait(bound)] => self.trait_bound(bound, source, form, presence),
            _ => Err(ScanError::unsupported_type(source)),
        }
    }

    fn trait_bound(
        &self,
        bound: &syn::TraitBound,
        source: &syn::Type,
        form: TraitUseForm,
        presence: HandlePresence,
    ) -> Result<TypeExpr, ScanError> {
        if !matches!(bound.modifier, syn::TraitBoundModifier::None) || bound.lifetimes.is_some() {
            return Err(ScanError::unsupported_type(source));
        }
        if bound
            .path
            .segments
            .iter()
            .any(|segment| !matches!(segment.arguments, syn::PathArguments::None))
        {
            return Err(ScanError::unsupported_type(source));
        }
        self.declared_trait(&bound.path, source)
            .map(|id| TypeExpr::r#trait(id, form, presence))
    }
}

fn is_unit(ty: &syn::Type) -> bool {
    matches!(unwrapped(ty), syn::Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn unwrapped(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Paren(paren) => unwrapped(&paren.elem),
        syn::Type::Group(group) => unwrapped(&group.elem),
        _ => ty,
    }
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
    use crate::ModulePath;
    use boltffi_ast::{ClassId, CustomTypeId, EnumId, RecordId, TraitId};

    fn ty(source: &str) -> syn::Type {
        syn::parse_str(source).expect("valid type")
    }

    fn scan(source: &str) -> Result<TypeExpr, ScanError> {
        Scanner::new(&DeclaredTypes::new(), &ModuleScope::root("demo")).scan(&ty(source))
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
    fn unwraps_parenthesized_types_before_scanning() {
        assert_eq!(scan("(i32)"), Ok(TypeExpr::Primitive(Primitive::I32)));
        assert_eq!(
            scan("Vec<(i32)>"),
            Ok(TypeExpr::vec(TypeExpr::Primitive(Primitive::I32)))
        );
        assert_eq!(scan("(())"), Ok(TypeExpr::Unit));
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
        let module = ModuleScope::new(ModulePath::root("demo").child("geometry"), &[]);
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
    fn resolves_registered_class_references_and_nullable_handles() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_class(ClassId::new("demo::Engine"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Engine")),
            Ok(TypeExpr::class(
                ClassId::new("demo::Engine"),
                HandlePresence::Required
            ))
        );
        assert_eq!(
            scanner.scan(&ty("Option<Engine>")),
            Ok(TypeExpr::class(
                ClassId::new("demo::Engine"),
                HandlePresence::Nullable
            ))
        );
    }

    #[test]
    fn resolves_registered_custom_remote_type() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(
            CustomTypeId::new("demo::UtcDateTime"),
            &ty("chrono::DateTime<chrono::Utc>"),
        );
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("chrono::DateTime<chrono::Utc>")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::UtcDateTime")))
        );
        assert_eq!(
            scanner.scan(&ty("DateTime<Utc>")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::UtcDateTime")))
        );
    }

    #[test]
    fn qualified_custom_remote_use_does_not_resolve_by_shape() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(
            CustomTypeId::new("demo::UtcDateTime"),
            &ty("chrono::DateTime<chrono::Utc>"),
        );
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert!(matches!(
            scanner.scan(&ty("other::DateTime<other::Utc>")),
            Err(ScanError::UnsupportedType { spelling })
                if spelling == "other::DateTime<other::Utc>"
        ));
    }

    #[test]
    fn resolves_imported_single_segment_custom_remote() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(CustomTypeId::new("demo::Uuid"), &ty("uuid::Uuid"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Uuid")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::Uuid")))
        );
    }

    #[test]
    fn custom_remote_shape_preserves_const_generic_arguments() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(CustomTypeId::new("demo::Array4"), &ty("fixed::Array<4>"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Array<4>")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::Array4")))
        );
        assert_eq!(
            scanner.scan(&ty("Array<8>")),
            Err(ScanError::UnsupportedType {
                spelling: "Array<const>".to_owned()
            })
        );
    }

    #[test]
    fn custom_remote_shape_preserves_associated_type_arguments() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(
            CustomTypeId::new("demo::DateIter"),
            &ty("iter::Iter<Item = chrono::DateTime<chrono::Utc>>"),
        );
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Iter<Item = DateTime<Utc>>")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::DateIter")))
        );
        assert!(matches!(
            scanner.scan(&ty("Iter<Item = String>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Iter<Item = String>"
        ));
    }

    #[test]
    fn ambiguous_custom_remote_shape_does_not_resolve_by_leaf_name() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(CustomTypeId::new("demo::FooId"), &ty("foo::Id"));
        declared_types.register_custom(CustomTypeId::new("demo::BarId"), &ty("bar::Id"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("foo::Id")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::FooId")))
        );
        assert_eq!(
            scanner.scan(&ty("bar::Id")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::BarId")))
        );
        assert!(matches!(
            scanner.scan(&ty("Id")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Id"
        ));
    }

    #[test]
    fn repeated_super_segments_resolve_custom_remotes() {
        let mut declared_types = DeclaredTypes::new();
        let custom_module = ModulePath::root("demo").child("api").child("v1");
        let call_module = ModulePath::root("demo").child("domain");
        let call_scope = ModuleScope::new(call_module, &[]);
        declared_types.register_custom_in(
            &custom_module,
            CustomTypeId::new("demo::api::v1::MoneyWire"),
            &ty("super::super::domain::Money"),
        );
        let scanner = Scanner::new(&declared_types, &call_scope);

        assert_eq!(
            scanner.scan(&ty("Money")),
            Ok(TypeExpr::Custom(CustomTypeId::new(
                "demo::api::v1::MoneyWire"
            )))
        );
    }

    #[test]
    fn resolves_custom_type_inside_containers_by_exact_remote() {
        let mut declared_types = DeclaredTypes::new();
        declared_types
            .register_custom(CustomTypeId::new("demo::UtcDateTime"), &ty("DateTime<Utc>"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Vec<DateTime<Utc>>")),
            Ok(TypeExpr::vec(TypeExpr::Custom(CustomTypeId::new(
                "demo::UtcDateTime"
            ))))
        );
        assert!(matches!(
            scanner.scan(&ty("DateTime")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "DateTime"
        ));
    }

    #[test]
    fn resolves_custom_remote_through_harmless_parentheses() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_custom(
            CustomTypeId::new("demo::UtcDateTime"),
            &ty("(DateTime<Utc>)"),
        );
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("DateTime<Utc>")),
            Ok(TypeExpr::Custom(CustomTypeId::new("demo::UtcDateTime")))
        );
    }

    #[test]
    fn declared_value_type_wins_over_matching_custom_remote() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Timestamp"));
        declared_types.register_custom(CustomTypeId::new("demo::TimestampWire"), &ty("Timestamp"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Timestamp")),
            Ok(TypeExpr::Record(RecordId::new("demo::Timestamp")))
        );
    }

    #[test]
    fn relative_custom_remote_resolution_uses_module_context() {
        let mut declared_types = DeclaredTypes::new();
        let custom_module = ModulePath::root("demo").child("custom");
        let data_module = ModulePath::root("demo").child("data");
        declared_types.register_custom_in(
            &custom_module,
            CustomTypeId::new("demo::custom::TimestampWire"),
            &ty("Timestamp"),
        );
        let custom_scope = ModuleScope::new(custom_module, &[]);
        let data_scope = ModuleScope::new(data_module, &[]);
        let custom_scanner = Scanner::new(&declared_types, &custom_scope);
        let data_scanner = Scanner::new(&declared_types, &data_scope);

        assert_eq!(
            custom_scanner.scan(&ty("Timestamp")),
            Ok(TypeExpr::Custom(CustomTypeId::new(
                "demo::custom::TimestampWire"
            )))
        );
        assert!(matches!(
            data_scanner.scan(&ty("Timestamp")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Timestamp"
        ));
    }

    #[test]
    fn qualified_custom_remote_resolution_crosses_module_context() {
        let mut declared_types = DeclaredTypes::new();
        let custom_module = ModulePath::root("demo").child("custom");
        let api_module = ModulePath::root("demo").child("api");
        declared_types.register_custom_in(
            &custom_module,
            CustomTypeId::new("demo::custom::UtcDateTime"),
            &ty("chrono::DateTime<chrono::Utc>"),
        );
        let api_scope = ModuleScope::new(api_module, &[]);
        let scanner = Scanner::new(&declared_types, &api_scope);

        assert_eq!(
            scanner.scan(&ty("chrono::DateTime<chrono::Utc>")),
            Ok(TypeExpr::Custom(CustomTypeId::new(
                "demo::custom::UtcDateTime"
            )))
        );
    }

    #[test]
    fn does_not_resolve_custom_binding_name_as_source_type() {
        let mut declared_types = DeclaredTypes::new();
        declared_types
            .register_custom(CustomTypeId::new("demo::UtcDateTime"), &ty("DateTime<Utc>"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert!(matches!(
            scanner.scan(&ty("UtcDateTime")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "UtcDateTime"
        ));
    }

    #[test]
    fn resolves_qualified_records_without_leaf_name_guessing() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::geometry::Point"));
        declared_types.register_record(RecordId::new("demo::Point"));
        declared_types.register_enum(EnumId::new("demo::geometry::Mode"));
        let module = ModuleScope::new(ModulePath::root("demo").child("shape"), &[]);
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
    fn rejects_named_type_arguments_before_erasing_them() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        declared_types.register_class(ClassId::new("demo::Engine"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert!(matches!(
            scanner.scan(&ty("Point<u32>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Point<u32>"
        ));
        assert!(matches!(
            scanner.scan(&ty("Option<Engine<u32>>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Engine<u32>"
        ));
        assert!(matches!(
            scanner.scan(&ty("i32<u32>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "i32<u32>"
        ));
    }

    #[test]
    fn self_type_is_captured_verbatim() {
        assert_eq!(scan("Self"), Ok(TypeExpr::SelfType));
    }

    #[test]
    fn resolves_callback_trait_use_forms() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_trait(TraitId::new("demo::Listener"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("impl Listener")),
            Ok(TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::ImplTrait,
                HandlePresence::Required,
            ))
        );
        assert_eq!(
            scanner.scan(&ty("Box<dyn Listener>")),
            Ok(TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::BoxedDyn,
                HandlePresence::Required,
            ))
        );
        assert_eq!(
            scanner.scan(&ty("std::sync::Arc<dyn Listener>")),
            Ok(TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::ArcDyn,
                HandlePresence::Required,
            ))
        );
    }

    #[test]
    fn folds_optional_callback_trait_handles_into_presence() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_trait(TraitId::new("demo::Listener"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert_eq!(
            scanner.scan(&ty("Option<Box<dyn Listener>>")),
            Ok(TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::BoxedDyn,
                HandlePresence::Nullable,
            ))
        );
        assert_eq!(
            scanner.scan(&ty("Option<std::sync::Arc<dyn Listener>>")),
            Ok(TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::ArcDyn,
                HandlePresence::Nullable,
            ))
        );
    }

    #[test]
    fn rejects_trait_references_that_would_erase_bounds_or_form() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_trait(TraitId::new("demo::Listener"));
        let module = ModuleScope::root("demo");
        let scanner = Scanner::new(&declared_types, &module);

        assert!(matches!(
            scanner.scan(&ty("Listener")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Listener"
        ));
        assert!(matches!(
            scanner.scan(&ty("impl Send + Listener")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "unrecognized type"
        ));
        assert!(matches!(
            scanner.scan(&ty("Box<dyn Listener + Send>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Box<dyn Listener + Send>"
        ));
        assert!(matches!(
            scanner.scan(&ty("impl Listener<i32>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "unrecognized type"
        ));
        assert!(matches!(
            scanner.scan(&ty("Box<dyn Listener<Item = i32>>")),
            Err(ScanError::UnsupportedType { spelling }) if spelling == "Box<dyn Listener<Item = i32>>"
        ));
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

use boltffi_ast::{FieldDef, RecordDef, RecordId};

use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::{ModulePath, ScanError, name, repr, visibility};

pub fn scan(
    marked: &Marked<'_, syn::ItemStruct>,
    declared_types: &DeclaredTypes,
) -> Result<RecordDef, ScanError> {
    let mut record = build(marked.item(), marked.module(), declared_types)?;
    marked.marker().append_value_attrs(&mut record.user_attrs);
    Ok(record)
}

fn build(
    item: &syn::ItemStruct,
    module: &ModulePath,
    declared_types: &DeclaredTypes,
) -> Result<RecordDef, ScanError> {
    let id = RecordId::new(module.qualified(&item.ident.to_string()));
    let mut record = RecordDef::new(id, name::canonical(&item.ident));
    record.repr = repr::scan(&item.attrs);
    record.source = visibility::scan(&item.vis);
    record.fields = record_fields(&item.fields, &Scanner::new(declared_types, module))?;
    Ok(record)
}

fn record_fields(fields: &syn::Fields, scanner: &Scanner<'_>) -> Result<Vec<FieldDef>, ScanError> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|field| record_field(field, scanner))
            .collect(),
        syn::Fields::Unnamed(_) | syn::Fields::Unit => Err(ScanError::TupleOrUnitStruct),
    }
}

fn record_field(field: &syn::Field, scanner: &Scanner<'_>) -> Result<FieldDef, ScanError> {
    let ident = field.ident.as_ref().ok_or(ScanError::TupleOrUnitStruct)?;
    let mut scanned = FieldDef::new(name::canonical(ident), scanner.scan(&field.ty)?);
    scanned.source = visibility::scan(&field.vis);
    Ok(scanned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        CanonicalName, FieldDef, NamePart, Primitive, ReprItem, Source, TypeExpr, Visibility,
    };

    fn parse(source: &str) -> syn::ItemStruct {
        syn::parse_str(source).expect("valid struct source")
    }

    fn scan(source: &str) -> Result<RecordDef, ScanError> {
        super::build(
            &parse(source),
            &ModulePath::root("demo"),
            &DeclaredTypes::new(),
        )
    }

    fn name(parts: &[&str]) -> CanonicalName {
        CanonicalName::new(parts.iter().copied().map(NamePart::new).collect())
    }

    #[test]
    fn scans_complete_named_field_record_contract() {
        let record = scan("pub struct Point { pub x: f64, pub y: f64 }").expect("scan");
        let mut expected = RecordDef::new(RecordId::new("demo::Point"), name(&["point"]));
        expected.source = Source::new(Visibility::Public, None);
        expected.fields = vec![
            FieldDef::new(name(&["x"]), TypeExpr::Primitive(Primitive::F64)),
            FieldDef::new(name(&["y"]), TypeExpr::Primitive(Primitive::F64)),
        ];

        assert_eq!(record, expected);
    }

    #[test]
    fn tuple_and_unit_structs_are_rejected() {
        let tuple = scan("pub struct Pair(i32, i32);").expect_err("tuple struct must reject");
        let unit = scan("pub struct Marker;").expect_err("unit struct must reject");

        assert_eq!(tuple, ScanError::TupleOrUnitStruct);
        assert_eq!(unit, ScanError::TupleOrUnitStruct);
    }

    #[test]
    fn scans_record_repr() {
        let record = scan("#[repr(C, align(8))] pub struct Point { pub x: f64 }").expect("scan");

        assert_eq!(record.repr.items, vec![ReprItem::C, ReprItem::Align(8)]);
    }

    #[test]
    fn scans_record_and_field_visibility() {
        let record = scan("pub(crate) struct Point { pub x: f64, y: f64 }").expect("scan");

        assert_eq!(
            record.source.visibility,
            Visibility::Restricted("crate".to_owned())
        );
        assert_eq!(record.fields[0].source.visibility, Visibility::Public);
        assert_eq!(record.fields[1].source.visibility, Visibility::Private);
        assert_eq!(record.fields[0].source.span, None);
        assert_eq!(record.fields[1].source.span, None);
    }

    #[test]
    fn scans_multi_word_names_as_parts() {
        let record = scan("pub struct HTTPRequest { pub user_id: i32 }").expect("scan");

        assert_eq!(record.id, RecordId::new("demo::HTTPRequest"));
        assert_eq!(record.name, name(&["http", "request"]));
        assert_eq!(record.fields[0].name, name(&["user", "id"]));
    }

    #[test]
    fn field_type_errors_keep_source_spelling() {
        let error = scan("pub struct Shape { pub point: Point }").expect_err("field type rejected");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn scans_string_and_collection_fields() {
        let record =
            scan("pub struct Person { pub name: String, pub scores: Vec<i32> }").expect("scan");

        assert_eq!(record.fields[0].type_expr, TypeExpr::String);
        assert_eq!(
            record.fields[1].type_expr,
            TypeExpr::vec(TypeExpr::Primitive(Primitive::I32))
        );
    }

    #[test]
    fn resolves_record_typed_field_against_registry() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let record = super::build(
            &parse("pub struct Shape { pub center: Point }"),
            &ModulePath::root("demo"),
            &declared_types,
        )
        .expect("scan");

        assert_eq!(
            record.fields[0].type_expr,
            TypeExpr::Record(RecordId::new("demo::Point"))
        );
    }
}

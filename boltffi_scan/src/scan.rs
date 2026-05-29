use std::path::Path as FsPath;

use boltffi_ast::{EnumDef, FunctionDef, PackageInfo, RecordDef, SourceContract, TraitDef};

use crate::declared_types::DeclaredTypes;
use crate::marked::MarkedItems;
use crate::source_tree::SourceTree;
use crate::{ScanError, items};

pub fn scan_source(
    path: impl AsRef<FsPath>,
    package: PackageInfo,
) -> Result<SourceContract, ScanError> {
    let source_tree = SourceTree::load(path.as_ref(), &package.name)?;
    scan_tree(source_tree, package)
}

pub fn scan_file(file: syn::File, package: PackageInfo) -> Result<SourceContract, ScanError> {
    let source_tree = SourceTree::inline(&package.name, file)?;
    scan_tree(source_tree, package)
}

fn scan_tree(source_tree: SourceTree, package: PackageInfo) -> Result<SourceContract, ScanError> {
    let marked = MarkedItems::collect(&source_tree)?;
    let declared_types = DeclaredTypes::index(&marked);
    let mut records = scan_records(&marked, &declared_types)?;
    let mut enums = scan_enums(&marked, &declared_types)?;
    let traits = scan_traits(&marked, &declared_types)?;
    items::impl_block::attach_methods(marked.impls(), &declared_types, &mut records, &mut enums)?;
    let functions = scan_functions(&marked, &declared_types)?;

    let mut contract = SourceContract::new(package);
    contract.records = records;
    contract.enums = enums;
    contract.traits = traits;
    contract.functions = functions;
    Ok(contract)
}

fn scan_records(
    marked: &MarkedItems<'_>,
    declared_types: &DeclaredTypes,
) -> Result<Vec<RecordDef>, ScanError> {
    marked
        .records()
        .iter()
        .map(|record| items::record::scan(record, declared_types))
        .collect()
}

fn scan_enums(
    marked: &MarkedItems<'_>,
    declared_types: &DeclaredTypes,
) -> Result<Vec<EnumDef>, ScanError> {
    marked
        .enums()
        .iter()
        .map(|enumeration| items::enumeration::scan(enumeration, declared_types))
        .collect()
}

fn scan_functions(
    marked: &MarkedItems<'_>,
    declared_types: &DeclaredTypes,
) -> Result<Vec<FunctionDef>, ScanError> {
    marked
        .functions()
        .iter()
        .map(|function| items::function::scan(function, declared_types))
        .collect()
}

fn scan_traits(
    marked: &MarkedItems<'_>,
    declared_types: &DeclaredTypes,
) -> Result<Vec<TraitDef>, ScanError> {
    marked
        .traits()
        .iter()
        .map(|callback| items::callback::scan(callback, declared_types))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{
        EnumId, HandlePresence, Primitive, Receiver, RecordId, ReturnDef, TraitId, TraitUseForm,
        TypeExpr,
    };

    fn parse(source: &str) -> syn::File {
        syn::parse_str(source).expect("valid source file")
    }

    fn try_scan(source: &str) -> Result<SourceContract, ScanError> {
        scan_file(parse(source), PackageInfo::new("demo", None))
    }

    fn scan(source: &str) -> SourceContract {
        try_scan(source).expect("scan")
    }

    fn point(contract: &SourceContract) -> &RecordDef {
        contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Point"))
            .expect("Point record")
    }

    #[test]
    fn scan_source_reads_and_parses_the_file_itself() {
        let path = std::env::temp_dir().join("boltffi_scan_entry_point.rs");
        std::fs::write(&path, "#[data] pub struct Point { pub x: f64 }").expect("write source");

        let contract = scan_source(&path, PackageInfo::new("demo", None)).expect("scan");

        std::fs::remove_file(&path).ok();
        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.records[0].id, RecordId::new("demo::Point"));
    }

    #[test]
    fn scan_source_reports_a_missing_file_as_a_read_error() {
        let path = std::env::temp_dir().join("boltffi_scan_does_not_exist.rs");
        std::fs::remove_file(&path).ok();

        let error = scan_source(&path, PackageInfo::new("demo", None))
            .expect_err("a missing file must reject");

        assert!(matches!(error, ScanError::Read { .. }));
    }

    #[test]
    fn scan_source_reports_invalid_rust_as_a_parse_error() {
        let path = std::env::temp_dir().join("boltffi_scan_invalid.rs");
        std::fs::write(&path, "#[data] pub struct {").expect("write source");

        let error = scan_source(&path, PackageInfo::new("demo", None))
            .expect_err("invalid source must reject");

        std::fs::remove_file(&path).ok();
        assert!(matches!(error, ScanError::Parse { .. }));
    }

    #[test]
    fn scans_items_across_modules_and_qualifies_ids_by_module_path() {
        let contract = scan(
            "#[data] pub struct Shape { pub center: crate::geometry::Point } \
             pub mod geometry { #[data] pub struct Point { pub x: f64 } }",
        );

        assert!(
            contract
                .records
                .iter()
                .any(|record| record.id == RecordId::new("demo::geometry::Point"))
        );
        let shape = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Shape"))
            .expect("Shape record");
        assert_eq!(
            shape.fields[0].type_expr,
            TypeExpr::Record(RecordId::new("demo::geometry::Point"))
        );
    }

    #[test]
    fn unqualified_reference_does_not_guess_across_modules() {
        let error = try_scan(
            "#[data] pub struct Shape { pub center: Point } \
             pub mod geometry { #[data] pub struct Point { pub x: f64 } }",
        )
        .expect_err("unqualified cross-module reference must reject");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }

    #[test]
    fn scans_marked_items_nested_several_modules_deep() {
        let contract = scan(
            "pub mod a { pub mod b { \
                 #[data] pub struct Deep { pub x: i32 } \
                 #[export] pub fn deep() -> Deep { todo!() } \
             } }",
        );

        assert_eq!(contract.records[0].id, RecordId::new("demo::a::b::Deep"));
        assert_eq!(
            contract.functions[0].returns,
            ReturnDef::Value(TypeExpr::Record(RecordId::new("demo::a::b::Deep")))
        );
    }

    #[test]
    fn resolves_record_reference_regardless_of_declaration_order() {
        let contract = scan(
            "#[data] pub struct Shape { pub center: Point } \
             #[data] pub struct Point { pub x: f64 }",
        );

        assert_eq!(contract.records.len(), 2);
        let shape = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Shape"))
            .expect("Shape record");
        assert_eq!(
            shape.fields[0].type_expr,
            TypeExpr::Record(RecordId::new("demo::Point"))
        );
    }

    #[test]
    fn scans_functions_and_resolves_their_record_references() {
        let contract = scan(
            "#[data] pub struct Point { pub x: f64 } \
             #[export] pub fn origin() -> Point { todo!() }",
        );

        assert_eq!(contract.functions.len(), 1);
        assert_eq!(
            contract.functions[0].returns,
            ReturnDef::Value(TypeExpr::Record(RecordId::new("demo::Point")))
        );
    }

    #[test]
    fn scans_exported_traits_and_resolves_callback_references() {
        let contract = scan(
            "#[export] pub trait Listener { fn on_value(&self, value: i32) -> i64; } \
             #[export] pub fn register(callback: impl Listener) {} \
             #[export] pub fn maybe_register(callback: Option<Box<dyn Listener>>) {}",
        );

        assert_eq!(contract.traits.len(), 1);
        assert_eq!(contract.traits[0].id, TraitId::new("demo::Listener"));
        assert_eq!(contract.traits[0].methods.len(), 1);
        assert_eq!(contract.traits[0].methods[0].receiver, Receiver::Shared);
        assert_eq!(
            contract.traits[0].methods[0].returns,
            ReturnDef::Value(TypeExpr::Primitive(Primitive::I64))
        );
        assert_eq!(
            contract.functions[0].parameters[0].type_expr,
            TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::ImplTrait,
                HandlePresence::Required,
            )
        );
        assert_eq!(
            contract.functions[1].parameters[0].type_expr,
            TypeExpr::r#trait(
                TraitId::new("demo::Listener"),
                TraitUseForm::BoxedDyn,
                HandlePresence::Nullable,
            )
        );
    }

    #[test]
    fn scans_enums_and_resolves_enum_typed_fields() {
        let contract = scan(
            "#[data] pub enum Mode { Fast, Slow } \
             #[data] pub struct Engine { pub mode: Mode }",
        );

        assert_eq!(contract.enums.len(), 1);
        assert_eq!(contract.enums[0].id, EnumId::new("demo::Mode"));
        let engine = contract
            .records
            .iter()
            .find(|record| record.id == RecordId::new("demo::Engine"))
            .expect("Engine record");
        assert_eq!(
            engine.fields[0].type_expr,
            TypeExpr::Enum(EnumId::new("demo::Mode"))
        );
    }

    #[test]
    fn attaches_impl_methods_to_their_record() {
        let contract = scan(
            "#[data] pub struct Point { pub x: f64, pub y: f64 } \
             #[data(impl)] impl Point { \
                 pub fn origin() -> Self { todo!() } \
                 pub fn distance(&self, other: Point) -> f64 { 0.0 } \
             }",
        );
        let point = point(&contract);

        assert_eq!(point.methods.len(), 2);
        assert_eq!(point.methods[0].receiver, Receiver::None);
        assert_eq!(
            point.methods[0].returns,
            ReturnDef::Value(TypeExpr::SelfType)
        );
        assert_eq!(point.methods[1].receiver, Receiver::Shared);
        assert_eq!(
            point.methods[1].parameters[0].type_expr,
            TypeExpr::Record(RecordId::new("demo::Point"))
        );
        assert_eq!(
            point.methods[1].returns,
            ReturnDef::Value(TypeExpr::Primitive(Primitive::F64))
        );
    }

    #[test]
    fn attaches_impl_methods_to_their_enum() {
        let contract = scan(
            "#[data] pub enum Mode { Fast, Slow } \
             #[data(impl)] impl Mode { \
                 pub fn parse(value: i32) -> Self { todo!() } \
             }",
        );

        assert_eq!(contract.enums[0].methods.len(), 1);
        assert_eq!(
            contract.enums[0].methods[0].returns,
            ReturnDef::Value(TypeExpr::SelfType)
        );
    }

    #[test]
    fn error_types_scan_as_value_types_and_preserve_the_error_attribute() {
        let contract = scan(
            "#[error] pub struct IoError { pub code: i32 } \
             #[error] pub enum ParseError { Eof, Unexpected }",
        );

        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.enums.len(), 1);

        let record = &contract.records[0];
        assert_eq!(record.id, RecordId::new("demo::IoError"));
        assert_eq!(record.user_attrs, vec![error_attr()]);

        let enumeration = &contract.enums[0];
        assert_eq!(enumeration.id, EnumId::new("demo::ParseError"));
        assert_eq!(enumeration.user_attrs, vec![error_attr()]);
    }

    #[test]
    fn data_types_carry_no_error_attribute() {
        let contract = scan("#[data] pub struct Point { pub x: f64 }");

        assert!(contract.records[0].user_attrs.is_empty());
    }

    #[test]
    fn references_to_error_types_resolve_like_any_value_type() {
        let contract = scan(
            "#[error] pub enum ParseError { Eof } \
             #[export] pub fn parse() -> Result<i32, ParseError> { todo!() }",
        );

        assert_eq!(
            contract.functions[0].returns,
            ReturnDef::Value(TypeExpr::Result {
                ok: Box::new(TypeExpr::Primitive(Primitive::I32)),
                err: Box::new(TypeExpr::Enum(EnumId::new("demo::ParseError"))),
            })
        );
    }

    fn error_attr() -> boltffi_ast::UserAttr {
        boltffi_ast::UserAttr::new(
            boltffi_ast::Path::single("error"),
            boltffi_ast::AttributeInput::Empty,
        )
    }

    #[test]
    fn unmarked_items_are_not_scanned() {
        let contract = scan(
            "pub struct Hidden { pub x: i32 } \
             pub enum Internal { A, B } \
             pub fn helper() {} \
             impl Hidden { pub fn touch(&self) {} }",
        );

        assert!(contract.records.is_empty());
        assert!(contract.enums.is_empty());
        assert!(contract.functions.is_empty());
    }

    #[test]
    fn qualified_markers_are_scanned() {
        let contract = scan(
            "#[boltffi::data] pub struct Point { pub x: f64 } \
             #[boltffi::export] pub fn origin() -> Point { todo!() }",
        );

        assert_eq!(contract.records.len(), 1);
        assert_eq!(contract.functions.len(), 1);
    }

    #[test]
    fn invalid_marker_arguments_are_rejected() {
        let error = try_scan("#[data(foo)] pub struct Point { pub x: f64 }")
            .expect_err("invalid marker argument must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarker {
                attribute: "data(foo)".to_owned()
            }
        );
    }

    #[test]
    fn marker_on_wrong_item_kind_is_rejected() {
        let error = try_scan("#[export] pub struct Point { pub x: f64 }")
            .expect_err("wrong marker placement must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "export".to_owned(),
                item: "struct".to_owned()
            }
        );
    }

    #[test]
    fn marker_on_module_is_rejected_after_module_loading() {
        let error = try_scan("#[data] pub mod geometry {}")
            .expect_err("wrong marker placement must reject");

        assert_eq!(
            error,
            ScanError::InvalidMarkerPlacement {
                marker: "data".to_owned(),
                item: "module".to_owned()
            }
        );
    }

    #[test]
    fn marked_impl_for_unknown_type_is_rejected() {
        let error = try_scan("#[data(impl)] impl Missing { pub fn run(&self) {} }")
            .expect_err("marked impl target must resolve");

        assert_eq!(
            error,
            ScanError::UnsupportedMarkedImpl {
                target: "Missing".to_owned()
            }
        );
    }

    #[test]
    fn non_declaration_items_are_ignored() {
        let contract =
            scan("use std::collections::HashMap; #[data] pub struct Point { pub x: f64 }");

        assert_eq!(contract.records.len(), 1);
        assert!(contract.functions.is_empty());
    }

    #[test]
    fn reference_to_unmarked_type_is_rejected() {
        let error = try_scan(
            "#[data] pub struct Shape { pub center: Point } \
             pub struct Point { pub x: f64 }",
        )
        .expect_err("reference to an unmarked type must reject");

        assert!(matches!(
            error,
            ScanError::UnsupportedType { spelling } if spelling == "Point"
        ));
    }
}

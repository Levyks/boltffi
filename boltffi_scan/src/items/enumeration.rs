use boltffi_ast::{EnumDef, EnumId, FieldDef, VariantDef, VariantPayload};

use crate::declared_types::DeclaredTypes;
use crate::marked::Marked;
use crate::type_expr::Scanner;
use crate::{ModulePath, ScanError, name, repr, visibility};

pub fn scan(
    marked: &Marked<'_, syn::ItemEnum>,
    declared_types: &DeclaredTypes,
) -> Result<EnumDef, ScanError> {
    let mut enumeration = build(marked.item(), marked.module(), declared_types)?;
    marked
        .marker()
        .append_value_attrs(&mut enumeration.user_attrs);
    Ok(enumeration)
}

fn build(
    item: &syn::ItemEnum,
    module: &ModulePath,
    declared_types: &DeclaredTypes,
) -> Result<EnumDef, ScanError> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(ScanError::UnsupportedGenerics {
            item: format!("enum {}", item.ident),
        });
    }
    let id = EnumId::new(module.qualified(&item.ident.to_string()));
    let mut enumeration = EnumDef::new(id, name::canonical(&item.ident));
    enumeration.repr = repr::scan(&item.attrs);
    enumeration.source = visibility::scan(&item.vis);
    let scanner = Scanner::new(declared_types, module);
    enumeration.variants = item
        .variants
        .iter()
        .map(|variant| variant_def(variant, &scanner))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(enumeration)
}

fn variant_def(variant: &syn::Variant, scanner: &Scanner<'_>) -> Result<VariantDef, ScanError> {
    let mut declaration = VariantDef::unit(name::canonical(&variant.ident));
    declaration.discriminant = discriminant(variant)?;
    declaration.payload = payload(&variant.fields, scanner)?;
    Ok(declaration)
}

fn payload(fields: &syn::Fields, scanner: &Scanner<'_>) -> Result<VariantPayload, ScanError> {
    match fields {
        syn::Fields::Unit => Ok(VariantPayload::Unit),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .map(|field| scanner.scan(&field.ty))
            .collect::<Result<Vec<_>, _>>()
            .map(VariantPayload::Tuple),
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|field| named_field(field, scanner))
            .collect::<Result<Vec<_>, _>>()
            .map(VariantPayload::Struct),
    }
}

fn named_field(field: &syn::Field, scanner: &Scanner<'_>) -> Result<FieldDef, ScanError> {
    let ident = field
        .ident
        .as_ref()
        .expect("named variant field carries an identifier");
    Ok(FieldDef::new(
        name::canonical(ident),
        scanner.scan(&field.ty)?,
    ))
}

fn discriminant(variant: &syn::Variant) -> Result<Option<i128>, ScanError> {
    match &variant.discriminant {
        None => Ok(None),
        Some((_, expr)) => discriminant_value(expr).map(Some),
    }
}

fn discriminant_value(expr: &syn::Expr) -> Result<i128, ScanError> {
    match expr {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(literal),
            ..
        }) => literal
            .base10_parse::<i128>()
            .map_err(|_| ScanError::UnsupportedDiscriminant),
        syn::Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => Ok(-discriminant_value(expr)?),
        _ => Err(ScanError::UnsupportedDiscriminant),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi_ast::{CanonicalName, NamePart, Primitive, RecordId, ReprItem, TypeExpr};

    fn parse(source: &str) -> syn::ItemEnum {
        syn::parse_str(source).expect("valid enum source")
    }

    fn scan(source: &str) -> Result<EnumDef, ScanError> {
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
    fn scans_unit_variants_with_repr_and_discriminants() {
        let enumeration = scan("#[repr(u8)] pub enum Mode { Fast = 0, Slow = 1 }").expect("scan");

        assert_eq!(enumeration.id, EnumId::new("demo::Mode"));
        assert_eq!(enumeration.name, name(&["mode"]));
        assert_eq!(
            enumeration.repr.items,
            vec![ReprItem::Primitive(Primitive::U8)]
        );
        assert_eq!(enumeration.variants.len(), 2);
        assert_eq!(enumeration.variants[0].name, name(&["fast"]));
        assert_eq!(enumeration.variants[0].discriminant, Some(0));
        assert_eq!(enumeration.variants[0].payload, VariantPayload::Unit);
        assert_eq!(enumeration.variants[1].discriminant, Some(1));
    }

    #[test]
    fn scans_tuple_and_struct_variant_payloads() {
        let mut declared_types = DeclaredTypes::new();
        declared_types.register_record(RecordId::new("demo::Point"));
        let enumeration = super::build(
            &parse("pub enum Shape { Dot(Point), Rect { width: f64, height: f64 } }"),
            &ModulePath::root("demo"),
            &declared_types,
        )
        .expect("scan");

        assert_eq!(
            enumeration.variants[0].payload,
            VariantPayload::Tuple(vec![TypeExpr::Record(RecordId::new("demo::Point"))])
        );
        match &enumeration.variants[1].payload {
            VariantPayload::Struct(fields) => {
                assert_eq!(fields[0].name, name(&["width"]));
                assert_eq!(fields[0].type_expr, TypeExpr::Primitive(Primitive::F64));
            }
            other => panic!("expected struct payload, got {other:?}"),
        }
    }

    #[test]
    fn negative_discriminant_is_captured() {
        let enumeration = scan("pub enum Sign { Neg = -1, Zero = 0 }").expect("scan");

        assert_eq!(enumeration.variants[0].discriminant, Some(-1));
        assert_eq!(enumeration.variants[1].discriminant, Some(0));
    }

    #[test]
    fn non_literal_discriminant_is_rejected() {
        let error = scan("pub enum Mask { A = 1 << 2 }").expect_err("non-literal must reject");

        assert_eq!(error, ScanError::UnsupportedDiscriminant);
    }

    #[test]
    fn rejects_generic_enum_before_erasing_type_parameters() {
        let error = scan("pub enum Boxed<T> { Value(T) }").expect_err("generic rejected");

        assert_eq!(
            error,
            ScanError::UnsupportedGenerics {
                item: "enum Boxed".to_owned()
            }
        );
    }
}

//! Constant declaration lowering.
//!
//! Walks every [`ConstantDef`] the source contract exposes and produces
//! a [`ConstantDecl<S>`] that names the constant, records the binding
//! [`TypeRef`] foreign code observes, and carries the literal value as
//! [`ConstantValueDecl::Inline`].
//!
//! Only inline literals are produced today. Constants whose source
//! expression cannot be expressed as a binding [`DefaultValue`] (paths,
//! tuples, arrays, floating-point or byte-string literals, raw
//! expressions) are rejected with [`UnsupportedType::DefaultValue`]
//! because the value-emission machinery does not yet model them and
//! [`ConstantValueDecl::Accessor`] needs a Rust-side reader symbol the
//! source AST does not carry.
//!
//! [`ConstantDef`]: boltffi_ast::ConstantDef
//! [`ConstantDecl<S>`]: crate::ConstantDecl
//! [`ConstantValueDecl::Inline`]: crate::ConstantValueDecl::Inline
//! [`ConstantValueDecl::Accessor`]: crate::ConstantValueDecl::Accessor
//! [`DefaultValue`]: crate::DefaultValue
//! [`TypeRef`]: crate::TypeRef
//! [`UnsupportedType::DefaultValue`]: super::error::UnsupportedType::DefaultValue

use boltffi_ast::{
    ConstExpr, ConstantDef as SourceConstant, Literal, Primitive as SourcePrimitive, TypeExpr,
};

use crate::{CanonicalName, ConstantDecl, ConstantValueDecl, DefaultValue, IntegerValue};

use super::{
    LowerError, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata,
    surface::SurfaceLower, types,
};

pub(super) fn lower<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
) -> Result<Vec<ConstantDecl<S>>, LowerError> {
    idx.constants()
        .iter()
        .map(|constant| lower_one::<S>(ids, constant))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    ids: &DeclarationIds,
    constant: &SourceConstant,
) -> Result<ConstantDecl<S>, LowerError> {
    let constant_id = ids.constant(&constant.id)?;
    let ty = types::lower(ids, &constant.type_expr)?;
    let value = inline_value(constant)?;
    Ok(ConstantDecl::new(
        constant_id,
        CanonicalName::from(&constant.name),
        metadata::decl_meta(constant.doc.as_ref(), constant.deprecated.as_ref()),
        ConstantValueDecl::inline(ty, value),
    ))
}

fn inline_value(constant: &SourceConstant) -> Result<DefaultValue, LowerError> {
    let Some(expected) = InlineConstantType::from_type_expr(&constant.type_expr) else {
        return Err(LowerError::unsupported_type(UnsupportedType::DefaultValue));
    };
    expected.lower_value(constant)
}

#[derive(Clone, Copy)]
enum InlineConstantType {
    Bool,
    Integer(IntegerBounds),
    String,
}

impl InlineConstantType {
    fn from_type_expr(type_expr: &TypeExpr) -> Option<Self> {
        match type_expr {
            TypeExpr::Primitive(SourcePrimitive::Bool) => Some(Self::Bool),
            TypeExpr::Primitive(primitive) => {
                IntegerBounds::for_primitive(*primitive).map(Self::Integer)
            }
            TypeExpr::String => Some(Self::String),
            _ => None,
        }
    }

    fn lower_value(self, constant: &SourceConstant) -> Result<DefaultValue, LowerError> {
        match (self, &constant.value) {
            (Self::Bool, ConstExpr::Literal(Literal::Bool(value))) => {
                Ok(DefaultValue::Bool(*value))
            }
            (Self::Integer(bounds), ConstExpr::Literal(Literal::Integer(value)))
                if bounds.contains(value.value) =>
            {
                Ok(DefaultValue::Integer(IntegerValue::new(value.value)))
            }
            (Self::String, ConstExpr::Literal(Literal::String(value))) => {
                Ok(DefaultValue::String(value.clone()))
            }
            (_, ConstExpr::Literal(Literal::Float(_) | Literal::Bytes(_)))
            | (_, ConstExpr::Path(_))
            | (_, ConstExpr::Array(_))
            | (_, ConstExpr::Tuple(_))
            | (_, ConstExpr::Raw(_)) => {
                Err(LowerError::unsupported_type(UnsupportedType::DefaultValue))
            }
            _ => Err(LowerError::invalid_constant_value(&constant.id)),
        }
    }
}

#[derive(Clone, Copy)]
struct IntegerBounds {
    min: i128,
    max: i128,
}

impl IntegerBounds {
    const fn new(min: i128, max: i128) -> Self {
        Self { min, max }
    }

    const fn for_primitive(primitive: SourcePrimitive) -> Option<Self> {
        match primitive {
            SourcePrimitive::I8 => Some(Self::new(i8::MIN as i128, i8::MAX as i128)),
            SourcePrimitive::U8 => Some(Self::new(0, u8::MAX as i128)),
            SourcePrimitive::I16 => Some(Self::new(i16::MIN as i128, i16::MAX as i128)),
            SourcePrimitive::U16 => Some(Self::new(0, u16::MAX as i128)),
            SourcePrimitive::I32 => Some(Self::new(i32::MIN as i128, i32::MAX as i128)),
            SourcePrimitive::U32 => Some(Self::new(0, u32::MAX as i128)),
            SourcePrimitive::I64 | SourcePrimitive::ISize => {
                Some(Self::new(i64::MIN as i128, i64::MAX as i128))
            }
            SourcePrimitive::U64 | SourcePrimitive::USize => Some(Self::new(0, u64::MAX as i128)),
            SourcePrimitive::Bool | SourcePrimitive::F32 | SourcePrimitive::F64 => None,
        }
    }

    const fn contains(self, value: i128) -> bool {
        self.min <= value && value <= self.max
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, ConstExpr, ConstantDef, ConstantId as SourceConstantId,
        DeprecationInfo as SourceDeprecationInfo, DocComment as SourceDocComment, FloatLiteral,
        IntegerLiteral, Literal, PackageInfo as SourcePackage, Path as SourcePath, Primitive,
        SourceContract, TypeExpr,
    };

    use crate::lower::{LowerError, LowerErrorKind, UnsupportedType, lower};
    use crate::{
        Bindings, CanonicalName, ConstantDecl, ConstantId, ConstantValueDecl, Decl, DefaultValue,
        IntegerValue, Native, Primitive as BindingPrimitive, SurfaceLower, TypeRef, Wasm32,
    };

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn constant(
        id: &str,
        constant_name: &str,
        type_expr: TypeExpr,
        value: ConstExpr,
    ) -> ConstantDef {
        ConstantDef::new(
            SourceConstantId::new(id),
            name(constant_name),
            type_expr,
            value,
        )
    }

    fn lower_constants<S: SurfaceLower>(
        constants: Vec<ConstantDef>,
    ) -> Result<Bindings<S>, LowerError> {
        let mut contract = package();
        contract.constants = constants;
        lower::<S>(&contract)
    }

    fn lower_constants_ok<S: SurfaceLower>(constants: Vec<ConstantDef>) -> Bindings<S> {
        lower_constants::<S>(constants).expect("constants should lower")
    }

    fn constant_decls<S: SurfaceLower>(bindings: &Bindings<S>) -> Vec<&ConstantDecl<S>> {
        bindings
            .decls()
            .iter()
            .filter_map(|decl| match decl {
                Decl::Constant(constant) => Some(constant.as_ref()),
                _ => None,
            })
            .collect()
    }

    fn only_constant<S: SurfaceLower>(bindings: &Bindings<S>) -> &ConstantDecl<S> {
        let decls = constant_decls(bindings);
        assert_eq!(decls.len(), 1, "expected exactly one constant declaration");
        decls[0]
    }

    #[test]
    fn bool_constant_lowers_inline_with_bool_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::ENABLED",
            "ENABLED",
            TypeExpr::Primitive(Primitive::Bool),
            ConstExpr::Literal(Literal::Bool(true)),
        )]);
        let decl = only_constant(&bindings);

        assert_eq!(decl.name(), &CanonicalName::single("ENABLED"));
        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::Bool));
                assert_eq!(value, &DefaultValue::Bool(true));
            }
            other => panic!("expected inline constant value, got {other:?}"),
        }
    }

    #[test]
    fn integer_constant_lowers_inline_with_integer_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::MAX_SIZE",
            "MAX_SIZE",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1024, "1024"))),
        )]);
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::U32));
                assert_eq!(value, &DefaultValue::Integer(IntegerValue::new(1024)));
            }
            other => panic!("expected inline integer constant, got {other:?}"),
        }
    }

    #[test]
    fn string_constant_lowers_inline_with_string_default() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::GREETING",
            "GREETING",
            TypeExpr::String,
            ConstExpr::Literal(Literal::String("hello".to_owned())),
        )]);
        let decl = only_constant(&bindings);

        match decl.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::String);
                assert_eq!(value, &DefaultValue::String("hello".to_owned()));
            }
            other => panic!("expected inline string constant, got {other:?}"),
        }
    }

    #[test]
    fn constant_literal_must_match_declared_type() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::ENABLED",
            "ENABLED",
            TypeExpr::String,
            ConstExpr::Literal(Literal::Bool(true)),
        )])
        .expect_err("mismatched constant literal must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::ENABLED"
        ));
    }

    #[test]
    fn integer_constant_must_fit_declared_type() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::BYTE",
            "BYTE",
            TypeExpr::Primitive(Primitive::U8),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(256, "256"))),
        )])
        .expect_err("out-of-range integer constant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::InvalidConstantValue(constant) if constant == "demo::BYTE"
        ));
    }

    #[test]
    fn constants_use_same_inline_value_on_wasm32() {
        let bindings = lower_constants_ok::<Wasm32>(vec![constant(
            "demo::MAX_SIZE",
            "MAX_SIZE",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1024, "1024"))),
        )]);

        match only_constant(&bindings).value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                assert_eq!(ty, &TypeRef::Primitive(BindingPrimitive::U32));
                assert_eq!(value, &DefaultValue::Integer(IntegerValue::new(1024)));
            }
            other => panic!("expected inline integer constant, got {other:?}"),
        }
    }

    #[test]
    fn float_constant_is_rejected_until_inline_floats_land() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::PI",
            "PI",
            TypeExpr::Primitive(Primitive::F64),
            ConstExpr::Literal(Literal::Float(FloatLiteral::new("3.14"))),
        )])
        .expect_err("float constant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::DefaultValue)
        ));
    }

    #[test]
    fn bytes_constant_is_rejected_until_inline_bytes_land() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::MAGIC",
            "MAGIC",
            TypeExpr::Bytes,
            ConstExpr::Literal(Literal::Bytes(vec![0xCA, 0xFE])),
        )])
        .expect_err("bytes constant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::DefaultValue)
        ));
    }

    #[test]
    fn path_constant_is_rejected_until_accessor_or_enum_variant_lands() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::ALIAS",
            "ALIAS",
            TypeExpr::String,
            ConstExpr::Path(SourcePath::single("OTHER")),
        )])
        .expect_err("path constant must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::DefaultValue)
        ));
    }

    #[test]
    fn raw_constant_is_rejected_until_accessor_lands() {
        let error = lower_constants::<Native>(vec![constant(
            "demo::COMPUTED",
            "COMPUTED",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Raw("1 + 2".to_owned()),
        )])
        .expect_err("raw constant expression must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::UnsupportedType(UnsupportedType::DefaultValue)
        ));
    }

    #[test]
    fn duplicate_constant_source_ids_are_rejected() {
        let error = lower_constants::<Native>(vec![
            constant(
                "demo::DUP",
                "DUP",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
            ),
            constant(
                "demo::DUP",
                "DUP_AGAIN",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ),
        ])
        .expect_err("duplicate constant id must reject");

        assert!(matches!(
            error.kind(),
            LowerErrorKind::DuplicateSourceId { .. }
        ));
    }

    #[test]
    fn multiple_constants_get_sequential_ids_in_source_order() {
        let bindings = lower_constants_ok::<Native>(vec![
            constant(
                "demo::ONE",
                "ONE",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(1, "1"))),
            ),
            constant(
                "demo::TWO",
                "TWO",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(2, "2"))),
            ),
            constant(
                "demo::THREE",
                "THREE",
                TypeExpr::Primitive(Primitive::U32),
                ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(3, "3"))),
            ),
        ]);
        let ids: Vec<u32> = constant_decls(&bindings)
            .into_iter()
            .map(|decl| decl.id().raw())
            .collect();

        assert_eq!(ids, vec![0, 1, 2]);
        assert_eq!(constant_decls(&bindings)[0].id(), ConstantId::from_raw(0));
    }

    #[test]
    fn constant_doc_and_deprecation_propagate_to_decl_meta() {
        let mut greeting = constant(
            "demo::GREETING",
            "GREETING",
            TypeExpr::String,
            ConstExpr::Literal(Literal::String("hello".to_owned())),
        );
        greeting.doc = Some(SourceDocComment::new("standard greeting"));
        greeting.deprecated = Some(SourceDeprecationInfo {
            note: Some("use GREETING_V2".to_owned()),
            since: Some("0.5".to_owned()),
        });

        let bindings = lower_constants_ok::<Native>(vec![greeting]);
        let meta = only_constant(&bindings).meta();

        assert_eq!(
            meta.doc().map(|doc| doc.as_str()),
            Some("standard greeting")
        );
        assert_eq!(
            meta.deprecated()
                .and_then(|deprecated| deprecated.message()),
            Some("use GREETING_V2")
        );
    }

    #[test]
    fn constant_does_not_register_any_native_symbols() {
        let bindings = lower_constants_ok::<Native>(vec![constant(
            "demo::MAX",
            "MAX",
            TypeExpr::Primitive(Primitive::U32),
            ConstExpr::Literal(Literal::Integer(IntegerLiteral::new(99, "99"))),
        )]);

        assert_eq!(bindings.symbols().symbols().len(), 0);
    }
}

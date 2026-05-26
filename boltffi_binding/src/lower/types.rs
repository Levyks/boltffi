use boltffi_ast::{ClosureType, ReturnDef, TypeExpr};

use crate::{ClosureTypeRef, Primitive, ReturnTypeRef, TypeRef};

use super::{LowerError, error::UnsupportedType, ids::DeclarationIds};

/// Lowers a source type expression into the [`TypeRef`] foreign code
/// sees on the boundary.
///
/// Walks the expression once, resolving every nested record, enum,
/// class, callback, and custom-type reference against the typed ids the
/// caller already built. Source shapes that have no IR encoding yet are
/// rejected here so callers can rely on a successful return for the
/// shape, not the codec.
pub(super) fn lower(ids: &DeclarationIds, type_expr: &TypeExpr) -> Result<TypeRef, LowerError> {
    Ok(match type_expr {
        TypeExpr::Primitive(primitive) => TypeRef::Primitive(Primitive::from(*primitive)),
        TypeExpr::String => TypeRef::String,
        TypeExpr::Bytes => TypeRef::Bytes,
        TypeExpr::Record(id) => TypeRef::Record(ids.record(id)?),
        TypeExpr::Enum(id) => TypeRef::Enum(ids.enumeration(id)?),
        TypeExpr::Class(id) => TypeRef::Class(ids.class(id)?),
        TypeExpr::Callback(id) => TypeRef::Callback(ids.callback(id)?),
        TypeExpr::Closure(closure) => TypeRef::Closure(Box::new(lower_closure(ids, closure)?)),
        TypeExpr::Custom(id) => TypeRef::Custom(ids.custom(id)?),
        TypeExpr::Vec(element) => TypeRef::Sequence(Box::new(lower(ids, element)?)),
        TypeExpr::Option(inner) => TypeRef::Optional(Box::new(lower(ids, inner)?)),
        TypeExpr::Tuple(elements) => TypeRef::Tuple(
            elements
                .iter()
                .map(|element| lower(ids, element))
                .collect::<Result<Vec<_>, LowerError>>()?,
        ),
        TypeExpr::Result { ok, err } => TypeRef::Result {
            ok: Box::new(lower(ids, ok)?),
            err: Box::new(lower(ids, err)?),
        },
        TypeExpr::Map { key, value } => TypeRef::Map {
            key: Box::new(lower(ids, key)?),
            value: Box::new(lower(ids, value)?),
        },
        TypeExpr::SelfType => {
            return Err(LowerError::unsupported_type(UnsupportedType::SelfType));
        }
        TypeExpr::Parameter(_) => {
            return Err(LowerError::unsupported_type(UnsupportedType::TypeParameter));
        }
    })
}

/// Lowers a source closure shape into a [`ClosureTypeRef`].
///
/// Shared between [`lower`] (for the [`TypeRef::Closure`] payload) and
/// the codec lane (for [`crate::CodecNode::ClosureHandle`]) so the two
/// always agree on the closure's parameter and return shape.
pub(super) fn lower_closure(
    ids: &DeclarationIds,
    closure: &ClosureType,
) -> Result<ClosureTypeRef, LowerError> {
    Ok(ClosureTypeRef::new(
        closure
            .parameters
            .iter()
            .map(|parameter| lower(ids, parameter))
            .collect::<Result<Vec<_>, LowerError>>()?,
        match &closure.returns {
            ReturnDef::Void => ReturnTypeRef::Void,
            ReturnDef::Value(TypeExpr::Result { .. }) => {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::FallibleClosureReturn,
                ));
            }
            ReturnDef::Value(value) => ReturnTypeRef::Value(lower(ids, value)?),
        },
    ))
}

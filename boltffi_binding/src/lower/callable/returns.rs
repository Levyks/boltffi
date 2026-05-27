use boltffi_ast::{ReturnDef, TypeExpr};

use crate::{
    Direction, ElementMeta, ErrorDecl, HandlePresence, HandleTarget, Primitive, ReturnDecl,
    ReturnPlan, TypeRef, ValueRef,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, records,
    surface::SurfaceLower, types,
};

use super::{CallableOwner, substitute_self_type};

/// The return and error pair produced by [`lower`] for one source
/// [`ReturnDef`].
pub(super) type Lowered<S, D> = (ReturnDecl<S, D>, ErrorDecl<S, D>);

/// Lowers a source [`ReturnDef`] into the pair the enclosing
/// [`CallableDecl`](crate::CallableDecl) records: a
/// [`ReturnDecl<S, D>`] for the success value and an
/// [`ErrorDecl<S, D>`] for the failure channel.
///
/// `D` is the enclosing scope's `K::ReturnDirection`. A `Result<T, E>`
/// return spills the success value into the out-pointer slot through
/// [`ReturnPlan::into_out`] and routes the error status through the
/// return slot. A `Result<(), E>` return produces a void success
/// channel paired with an encoded error channel.
pub(super) fn lower<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    owner: CallableOwner<'_>,
    return_def: &ReturnDef,
) -> Result<Lowered<S, D>, LowerError> {
    match return_def {
        ReturnDef::Void => Ok((
            ReturnDecl::new(ElementMeta::new(None, None, None), ReturnPlan::Void),
            ErrorDecl::none(),
        )),
        ReturnDef::Value(type_expr) => {
            if let TypeExpr::Result { ok, err } = type_expr {
                let ok_type_expr = substitute_self_type(owner, ok)?;
                let err_type_expr = substitute_self_type(owner, err)?;
                let success = lower_return_plan::<S, D>(idx, ids, &ok_type_expr)?.into_out();
                let error = lower_error::<S, D>(idx, ids, &err_type_expr)?;
                return Ok((
                    ReturnDecl::new(ElementMeta::new(None, None, None), success),
                    error,
                ));
            }
            if matches!(type_expr, TypeExpr::Unit) {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::UnitInValuePosition,
                ));
            }
            let type_expr = substitute_self_type(owner, type_expr)?;
            let plan = lower_plain_return::<S, D>(idx, ids, &type_expr)?;
            Ok((
                ReturnDecl::new(ElementMeta::new(None, None, None), plan),
                ErrorDecl::none(),
            ))
        }
    }
}

fn lower_plain_return<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<ReturnPlan<S, D>, LowerError> {
    match specialize_return::<S, D>(idx, ids, type_expr)? {
        Some(plan) => Ok(plan),
        None => lower_return_plan::<S, D>(idx, ids, type_expr),
    }
}

fn specialize_return<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<Option<ReturnPlan<S, D>>, LowerError> {
    Ok(match type_expr {
        TypeExpr::Option(inner) => {
            primitive(inner).map(|primitive| ReturnPlan::ScalarOptionViaReturnSlot { primitive })
        }
        TypeExpr::Vec(inner) => direct_vec_element(idx, ids, inner)?
            .map(|element| ReturnPlan::DirectVecViaReturnSlot { element }),
        _ => None,
    })
}

fn primitive(type_expr: &TypeExpr) -> Option<Primitive> {
    if let TypeExpr::Primitive(p) = type_expr {
        Some(Primitive::from(*p))
    } else {
        None
    }
}

fn direct_vec_element(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<Option<TypeRef>, LowerError> {
    match type_expr {
        TypeExpr::Primitive(_) => Ok(Some(types::lower(ids, type_expr)?)),
        TypeExpr::Record(id) if idx.record(id).is_some_and(records::is_direct) => {
            Ok(Some(types::lower(ids, type_expr)?))
        }
        _ => Ok(None),
    }
}

fn lower_error<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<ErrorDecl<S, D>, LowerError> {
    let ty = types::lower(ids, type_expr)?;
    let codec_node = codecs::node(idx, ids, type_expr, ValueRef::self_value())?;
    Ok(ErrorDecl::EncodedViaReturnSlot {
        ty,
        codec: D::make_codec(ValueRef::self_value(), codec_node),
        shape: S::encoded_return_shape(),
    })
}

/// Picks the [`ReturnPlan<S, D>`] for one return value.
///
/// Emits `*ViaReturnSlot` variants by default. Result returns rewrite
/// the plan with [`ReturnPlan::into_out`] so the success value spills
/// to an out-pointer and the error channel can claim the return slot.
///
/// [`TypeExpr::Unit`] lowers to [`ReturnPlan::Void`] so that
/// `Result<(), E>` produces a void success channel paired with the
/// error channel without routing an empty value through the codec lane.
fn lower_return_plan<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<ReturnPlan<S, D>, LowerError> {
    match type_expr {
        TypeExpr::Unit => Ok(ReturnPlan::Void),
        TypeExpr::Primitive(_) => Ok(ReturnPlan::DirectViaReturnSlot {
            ty: types::lower(ids, type_expr)?,
        }),
        TypeExpr::Record(id) if idx.record(id).is_some_and(records::is_direct) => {
            Ok(ReturnPlan::DirectViaReturnSlot {
                ty: types::lower(ids, type_expr)?,
            })
        }
        TypeExpr::Enum(id) if idx.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(ReturnPlan::DirectViaReturnSlot {
                ty: types::lower(ids, type_expr)?,
            })
        }
        TypeExpr::String
        | TypeExpr::Bytes
        | TypeExpr::Record(_)
        | TypeExpr::Enum(_)
        | TypeExpr::Vec(_)
        | TypeExpr::Option(_)
        | TypeExpr::Tuple(_)
        | TypeExpr::Result { .. }
        | TypeExpr::Map { .. }
        | TypeExpr::Custom(_) => {
            let ty = types::lower(ids, type_expr)?;
            let codec_node = codecs::node(idx, ids, type_expr, ValueRef::self_value())?;
            Ok(ReturnPlan::EncodedViaReturnSlot {
                ty,
                codec: D::make_codec(ValueRef::self_value(), codec_node),
                shape: S::encoded_return_shape(),
            })
        }
        TypeExpr::Closure(_) => Err(LowerError::unsupported_type(UnsupportedType::ClosureReturn)),
        TypeExpr::Class { id, presence } => Ok(ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(ids.class(id)?),
            carrier: S::class_handle_carrier(),
            presence: lower_presence(*presence),
        }),
        TypeExpr::Trait {
            id,
            form: _,
            presence,
        } => Ok(ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Callback(ids.callback(id)?),
            carrier: S::callback_handle_carrier(),
            presence: lower_presence(*presence),
        }),
        TypeExpr::SelfType | TypeExpr::Parameter(_) => {
            Err(types::lower(ids, type_expr).expect_err("unsupported value-position type expr"))
        }
    }
}

fn lower_presence(presence: boltffi_ast::HandlePresence) -> HandlePresence {
    match presence {
        boltffi_ast::HandlePresence::Required => HandlePresence::Required,
        boltffi_ast::HandlePresence::Nullable => HandlePresence::Nullable,
    }
}

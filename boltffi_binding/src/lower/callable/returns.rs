use boltffi_ast::{ReturnDef, TypeExpr};

use crate::{
    Direction, ElementMeta, ErrorDecl, HandleTarget, ParamDirection, Primitive, ReturnDecl,
    ReturnPlan, TypeRef, ValueRef,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, records,
    surface::SurfaceLower, symbol::SymbolAllocator, types,
};

use super::{
    CallableOwner, CallbackHandleSource, ClassHandleSource, ClosureSource, params::LowerClosure,
    substitute_self_type,
};

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
///
/// Closure returns dispatch through [`LowerClosure`] (the trait
/// also covers the return position because the closure crossing shape
/// is the same in either slot), so the direction `D` decides
/// structurally whether the invoke contract is foreign- or
/// Rust-implemented.
pub(super) fn lower<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner<'_>,
    return_def: &ReturnDef,
) -> Result<Lowered<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    match return_def {
        ReturnDef::Void => Ok((
            ReturnDecl::new(ElementMeta::new(None, None, None), ReturnPlan::Void),
            ErrorDecl::none(),
        )),
        ReturnDef::Value(type_expr) => {
            if let TypeExpr::Result { ok, err } = type_expr {
                let ok_type_expr = substitute_self_type(owner, ok)?;
                let err_type_expr = substitute_self_type(owner, err)?;
                let success =
                    lower_return_plan::<S, D>(idx, ids, allocator, &ok_type_expr)?.into_out();
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
            let plan = lower_plain_return::<S, D>(idx, ids, allocator, &type_expr)?;
            Ok((
                ReturnDecl::new(ElementMeta::new(None, None, None), plan),
                ErrorDecl::none(),
            ))
        }
    }
}

fn lower_plain_return<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    type_expr: &TypeExpr,
) -> Result<ReturnPlan<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    match specialize_return::<S, D>(idx, ids, type_expr)? {
        Some(plan) => Ok(plan),
        None => lower_return_plan::<S, D>(idx, ids, allocator, type_expr),
    }
}

fn specialize_return<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<Option<ReturnPlan<S, D>>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
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
        TypeExpr::Primitive(_) if !types::is_byte_primitive(type_expr) => {
            Ok(Some(types::lower(ids, type_expr)?))
        }
        TypeExpr::Record { id, .. } if idx.record(id).is_some_and(records::is_direct) => {
            Ok(Some(types::lower(ids, type_expr)?))
        }
        _ => Ok(None),
    }
}

fn lower_error<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<ErrorDecl<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    let ty = types::lower(ids, type_expr)?;
    let codec_node = codecs::node(idx, ids, type_expr, ValueRef::self_value())?;
    Ok(ErrorDecl::EncodedViaReturnSlot {
        ty,
        codec: D::make_codec(ValueRef::self_value(), codec_node),
        shape: S::encoded_return_shape(),
    })
}

fn lower_return_plan<S: SurfaceLower, D: Direction + LowerClosure<S>>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    type_expr: &TypeExpr,
) -> Result<ReturnPlan<S, D>, LowerError>
where
    D::Opposite: ParamDirection<S>,
{
    if let Some(handle) = ClassHandleSource::from_type_expr(type_expr) {
        return Ok(ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Class(ids.class(handle.id)?),
            carrier: S::class_handle_carrier(),
            presence: handle.presence,
        });
    }
    if let Some(handle) = CallbackHandleSource::from_type_expr(type_expr) {
        return Ok(ReturnPlan::HandleViaReturnSlot {
            target: HandleTarget::Callback(ids.callback(handle.id)?),
            carrier: S::callback_handle_carrier(),
            presence: handle.presence,
        });
    }
    if let Some(closure) = ClosureSource::from_type_expr(type_expr) {
        let closure_return = D::lower_closure_return(idx, ids, allocator, closure)?;
        return Ok(ReturnPlan::ClosureViaOutPointer(closure_return));
    }
    match type_expr {
        TypeExpr::Unit => Ok(ReturnPlan::Void),
        TypeExpr::Primitive(_) => Ok(ReturnPlan::DirectViaReturnSlot {
            ty: types::lower(ids, type_expr)?,
        }),
        TypeExpr::Record { id, .. } if idx.record(id).is_some_and(records::is_direct) => {
            Ok(ReturnPlan::DirectViaReturnSlot {
                ty: types::lower(ids, type_expr)?,
            })
        }
        TypeExpr::Enum { id, .. } if idx.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(ReturnPlan::DirectViaReturnSlot {
                ty: types::lower(ids, type_expr)?,
            })
        }
        TypeExpr::String
        | TypeExpr::Str
        | TypeExpr::Builtin(_)
        | TypeExpr::Slice(_)
        | TypeExpr::Record { .. }
        | TypeExpr::Enum { .. }
        | TypeExpr::Vec(_)
        | TypeExpr::Option(_)
        | TypeExpr::Tuple(_)
        | TypeExpr::Result { .. }
        | TypeExpr::Map { .. }
        | TypeExpr::Custom { .. } => {
            let ty = types::lower(ids, type_expr)?;
            let codec_node = codecs::node(idx, ids, type_expr, ValueRef::self_value())?;
            Ok(ReturnPlan::EncodedViaReturnSlot {
                ty,
                codec: D::make_codec(ValueRef::self_value(), codec_node),
                shape: S::encoded_return_shape(),
            })
        }
        TypeExpr::SelfType
        | TypeExpr::Parameter(_)
        | TypeExpr::Class { .. }
        | TypeExpr::FnPtr(_)
        | TypeExpr::ImplTrait(_)
        | TypeExpr::Dyn(_)
        | TypeExpr::Boxed(_)
        | TypeExpr::Arc(_) => {
            Err(types::lower(ids, type_expr).expect_err(
                "return value-plan lowering reached a source type reserved for handle, closure, owner-substitution, or generic rejection before the direct/encoded fallback",
            ))
        }
    }
}

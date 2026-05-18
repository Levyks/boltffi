use boltffi_ast::{ReturnDef, TypeExpr};

use crate::{ElementMeta, ErrorDecl, HandleTarget, LiftPlan, ReadPlan, ReturnDecl, ValueRef};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, records,
    surface::SurfaceLower, types,
};

use super::{CallableOwner, substitute_self_type};

/// Lowers a source [`ReturnDef`] into the IR pair the surrounding
/// [`CallableDecl`] records: a [`ReturnDecl<S>`] for the success
/// value and an [`ErrorDecl<S>`] for the failure channel.
///
/// [`CallableDecl`]: crate::CallableDecl
pub(super) fn lower<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    owner: CallableOwner<'_>,
    return_def: &ReturnDef,
) -> Result<(ReturnDecl<S>, ErrorDecl<S>), LowerError> {
    match return_def {
        ReturnDef::Void => Ok((
            ReturnDecl::new(ElementMeta::new(None, None, None), LiftPlan::Void),
            ErrorDecl::none(),
        )),
        ReturnDef::Value(type_expr) => {
            if let TypeExpr::Result { ok, err } = type_expr {
                let ok_type_expr = substitute_self_type(owner, ok);
                let err_type_expr = substitute_self_type(owner, err);
                let lift = lower_lift::<S>(idx, ids, &ok_type_expr)?.into_out();
                let error = lower_error::<S>(idx, ids, &err_type_expr)?;
                return Ok((
                    ReturnDecl::new(ElementMeta::new(None, None, None), lift),
                    error,
                ));
            }
            let type_expr = substitute_self_type(owner, type_expr);
            let lift = lower_lift::<S>(idx, ids, &type_expr)?;
            Ok((
                ReturnDecl::new(ElementMeta::new(None, None, None), lift),
                ErrorDecl::none(),
            ))
        }
    }
}

fn lower_error<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<ErrorDecl<S>, LowerError> {
    Ok(ErrorDecl::EncodedReturn {
        ty: types::lower(ids, type_expr)?,
        read: ReadPlan::new(codecs::node(idx, ids, type_expr, ValueRef::self_value())?),
        shape: S::encoded_return_shape(),
    })
}

/// Picks the [`LiftPlan`] for one return value from its source type.
///
/// Mirrors the parameter-side dispatch but emits lift-side IR
/// variants. Out-pointer variants activate when a `Result<T, E>`
/// return gives the native return slot to the error channel.
fn lower_lift<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
) -> Result<LiftPlan<S>, LowerError> {
    match type_expr {
        TypeExpr::Primitive(_) => Ok(LiftPlan::Direct {
            ty: types::lower(ids, type_expr)?,
        }),
        TypeExpr::Record(id) if idx.record(id).is_some_and(records::is_direct) => {
            Ok(LiftPlan::Direct {
                ty: types::lower(ids, type_expr)?,
            })
        }
        TypeExpr::Enum(id) if idx.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(LiftPlan::Direct {
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
        | TypeExpr::Map { .. } => {
            let ty = types::lower(ids, type_expr)?;
            let codec = codecs::node(idx, ids, type_expr, ValueRef::self_value())?;
            Ok(LiftPlan::Encoded {
                ty,
                read: ReadPlan::new(codec),
                shape: S::encoded_return_shape(),
            })
        }
        TypeExpr::Closure(closure) => Ok(LiftPlan::Handle {
            target: HandleTarget::Closure(Box::new(types::lower_closure(ids, closure)?)),
            carrier: S::closure_handle_carrier(),
        }),
        TypeExpr::Class(id) => Ok(LiftPlan::Handle {
            target: HandleTarget::Class(ids.class(id)?),
            carrier: S::class_handle_carrier(),
        }),
        TypeExpr::Callback(_) | TypeExpr::Custom(_) => {
            let _ = types::lower(ids, type_expr)?;
            Err(LowerError::unsupported_type(UnsupportedType::SelfType))
        }
        TypeExpr::SelfType | TypeExpr::Parameter(_) => {
            Err(types::lower(ids, type_expr).expect_err("unsupported value-position type expr"))
        }
    }
}

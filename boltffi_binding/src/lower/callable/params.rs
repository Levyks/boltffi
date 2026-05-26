use boltffi_ast::{ParameterDef, ParameterPassing, TypeExpr};

use crate::{
    CanonicalName, HandlePresence, HandleTarget, LowerPlan, ParamDecl, Receive, ValueRef, WritePlan,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata,
    records, surface::SurfaceLower, types,
};

use super::{CallableOwner, substitute_self_type};

pub(super) fn lower<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    owner: CallableOwner<'_>,
    parameters: &[ParameterDef],
) -> Result<Vec<ParamDecl<S>>, LowerError> {
    parameters
        .iter()
        .map(|parameter| lower_one::<S>(idx, ids, owner, parameter))
        .collect()
}

fn lower_one<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    owner: CallableOwner<'_>,
    parameter: &ParameterDef,
) -> Result<ParamDecl<S>, LowerError> {
    let type_expr = substitute_self_type(owner, &parameter.type_expr)?;
    let receive = receive_for_passing(parameter.passing);
    let canonical_name = CanonicalName::from(&parameter.name);
    let value = ValueRef::named(canonical_name.clone());
    let plan = lower_plan::<S>(idx, ids, &type_expr, value, receive)?;
    let meta = metadata::element_meta(parameter.doc.as_ref(), None, parameter.default.as_ref())?;
    Ok(ParamDecl::new(canonical_name, meta, plan))
}

fn receive_for_passing(passing: ParameterPassing) -> Receive {
    match passing {
        ParameterPassing::Value => Receive::ByValue,
        ParameterPassing::Ref => Receive::ByRef,
        ParameterPassing::RefMut => Receive::ByMutRef,
    }
}

/// Picks the [`LowerPlan`] for one parameter from its source type.
///
/// Each handle-shaped [`TypeExpr`] carries the dimensions the boundary
/// needs (callback presence, class identity, closure signature) directly
/// in its variant. The lowering pass is a structural transform with no
/// `(passing, type_expr)` cross-product: source like
/// `Option<Box<dyn Listener>>` has already collapsed into
/// `TypeExpr::Trait { form: BoxedDyn, presence: Nullable }` at the
/// scanner, and the surface spelling
/// [`TraitUseForm`](boltffi_ast::TraitUseForm) is invisible here
/// because the wire carrier is identical across the supported forms.
fn lower_plan<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    value: ValueRef,
    receive: Receive,
) -> Result<LowerPlan<S>, LowerError> {
    match type_expr {
        TypeExpr::Primitive(_) => Ok(LowerPlan::Direct {
            ty: types::lower(ids, type_expr)?,
            receive,
        }),
        TypeExpr::Record(id) if idx.record(id).is_some_and(records::is_direct) => {
            Ok(LowerPlan::Direct {
                ty: types::lower(ids, type_expr)?,
                receive,
            })
        }
        TypeExpr::Enum(id) if idx.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(LowerPlan::Direct {
                ty: types::lower(ids, type_expr)?,
                receive,
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
            let codec = codecs::node(idx, ids, type_expr, value.clone())?;
            Ok(LowerPlan::Encoded {
                ty,
                write: WritePlan::new(value, codec),
                shape: S::encoded_param_shape(),
                receive,
            })
        }
        TypeExpr::Closure(closure) => Ok(LowerPlan::Handle {
            target: HandleTarget::Closure(Box::new(types::lower_closure(ids, closure)?)),
            carrier: S::closure_handle_carrier(),
            receive,
            presence: HandlePresence::Required,
        }),
        TypeExpr::Class(id) => Ok(LowerPlan::Handle {
            target: HandleTarget::Class(ids.class(id)?),
            carrier: S::class_handle_carrier(),
            receive,
            presence: HandlePresence::Required,
        }),
        TypeExpr::Trait {
            id,
            form: _,
            presence,
        } => {
            if !matches!(receive, Receive::ByValue) {
                return Err(LowerError::unsupported_type(
                    UnsupportedType::BorrowedCallbackParameter,
                ));
            }
            Ok(LowerPlan::Handle {
                target: HandleTarget::Callback(ids.callback(id)?),
                carrier: S::callback_handle_carrier(),
                receive,
                presence: lower_presence(*presence),
            })
        }
        TypeExpr::Custom(_) => Err(types::lower(ids, type_expr)
            .err()
            .unwrap_or_else(|| LowerError::unsupported_type(UnsupportedType::SelfType))),
        TypeExpr::Unit | TypeExpr::SelfType | TypeExpr::Parameter(_) => {
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

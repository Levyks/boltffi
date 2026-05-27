use boltffi_ast::{ClosureType, ParameterDef, ParameterPassing, TypeExpr};

use crate::{
    CanonicalName, Direction, ElementMeta, HandlePresence, HandleTarget, IntoRust, OutOfRust,
    ParamDecl, ParamDirection, ParamPlan, Primitive, Receive, TypeRef, ValueRef,
};

use super::super::{
    LowerError, codecs, enums, error::UnsupportedType, ids::DeclarationIds, index::Index, metadata,
    records, surface::SurfaceLower, symbol::SymbolAllocator, types,
};

use super::{CallableOwner, substitute_self_type};

/// Lowers the parameter list of a callable in direction `D`.
///
/// The caller picks `D` from the enclosing scope's `ParamDirection`:
/// [`IntoRust`](crate::IntoRust) for parameters of a Rust-implemented
/// callable, [`OutOfRust`](crate::OutOfRust) for parameters of a
/// foreign-implemented callable.
///
/// Value parameters lower through [`ParamPlan`]. Closure parameters
/// dispatch through the [`ClosureParamSlot`] trait so each direction
/// supplies the AST-to-IR construction for its closure shape; both
/// directions yield [`ClosureParam<S, D>`](crate::ClosureParam), whose
/// invoke contract resolves to [`ImportedCallable<S>`] for `IntoRust`
/// and [`ExportedCallable<S>`] for `OutOfRust` through
/// [`Direction::InvokeScope`](crate::Direction::InvokeScope).
///
/// [`ImportedCallable<S>`]: crate::ImportedCallable
/// [`ExportedCallable<S>`]: crate::ExportedCallable
pub(super) fn lower<S: SurfaceLower, D: Direction + ClosureParamSlot<S>>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner<'_>,
    parameters: &[ParameterDef],
) -> Result<Vec<ParamDecl<S, D>>, LowerError> {
    parameters
        .iter()
        .map(|parameter| lower_one::<S, D>(idx, ids, allocator, owner, parameter))
        .collect()
}

fn lower_one<S: SurfaceLower, D: Direction + ClosureParamSlot<S>>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner<'_>,
    parameter: &ParameterDef,
) -> Result<ParamDecl<S, D>, LowerError> {
    let type_expr = substitute_self_type(owner, &parameter.type_expr)?;
    let receive = receive_for_passing(parameter.passing);
    let canonical_name = CanonicalName::from(&parameter.name);
    let meta = metadata::element_meta(parameter.doc.as_ref(), None, parameter.default.as_ref())?;
    if let TypeExpr::Closure(closure) = &type_expr {
        if !matches!(receive, Receive::ByValue) {
            return Err(LowerError::unsupported_type(
                UnsupportedType::BorrowedCallbackParameter,
            ));
        }
        return D::lower_closure_param(idx, ids, allocator, canonical_name, meta, closure);
    }
    let value = ValueRef::named(canonical_name.clone());
    let plan = lower_plain_plan::<S, D>(idx, ids, &type_expr, value, receive)?;
    Ok(ParamDecl::value(canonical_name, meta, plan))
}

fn lower_plain_plan<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    value: ValueRef,
    receive: Receive,
) -> Result<ParamPlan<S, D>, LowerError> {
    match specialize_param::<S, D>(idx, ids, type_expr, receive)? {
        Some(plan) => Ok(plan),
        None => lower_plan::<S, D>(idx, ids, type_expr, value, receive),
    }
}

fn specialize_param<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    receive: Receive,
) -> Result<Option<ParamPlan<S, D>>, LowerError> {
    if !matches!(receive, Receive::ByValue) {
        return Ok(None);
    }
    Ok(match type_expr {
        TypeExpr::Option(inner) => {
            primitive(inner).map(|primitive| ParamPlan::ScalarOption { primitive })
        }
        TypeExpr::Vec(inner) => {
            direct_vec_element(idx, ids, inner)?.map(|element| ParamPlan::DirectVec { element })
        }
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

fn receive_for_passing(passing: ParameterPassing) -> Receive {
    match passing {
        ParameterPassing::Value => Receive::ByValue,
        ParameterPassing::Ref => Receive::ByRef,
        ParameterPassing::RefMut => Receive::ByMutRef,
    }
}

fn lower_plan<S: SurfaceLower, D: Direction>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    type_expr: &TypeExpr,
    value: ValueRef,
    receive: Receive,
) -> Result<ParamPlan<S, D>, LowerError> {
    match type_expr {
        TypeExpr::Primitive(_) => Ok(ParamPlan::Direct {
            ty: types::lower(ids, type_expr)?,
            receive: D::receive_from(receive),
        }),
        TypeExpr::Record(id) if idx.record(id).is_some_and(records::is_direct) => {
            Ok(ParamPlan::Direct {
                ty: types::lower(ids, type_expr)?,
                receive: D::receive_from(receive),
            })
        }
        TypeExpr::Enum(id) if idx.enumeration(id).is_some_and(enums::is_c_style) => {
            Ok(ParamPlan::Direct {
                ty: types::lower(ids, type_expr)?,
                receive: D::receive_from(receive),
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
            let codec_node = codecs::node(idx, ids, type_expr, value.clone())?;
            Ok(ParamPlan::Encoded {
                ty,
                codec: D::make_codec(value, codec_node),
                shape: S::encoded_param_shape(),
                receive: D::receive_from(receive),
            })
        }
        TypeExpr::Closure(_) => unreachable!("closures are handled before lower_plan"),
        TypeExpr::Class { id, presence } => Ok(ParamPlan::Handle {
            target: HandleTarget::Class(ids.class(id)?),
            carrier: S::class_handle_carrier(),
            presence: lower_presence(*presence),
            receive: D::receive_from(receive),
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
            Ok(ParamPlan::Handle {
                target: HandleTarget::Callback(ids.callback(id)?),
                carrier: S::callback_handle_carrier(),
                presence: lower_presence(*presence),
                receive: D::receive_from(receive),
            })
        }
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

/// Lowers a closure parameter at one direction.
///
/// Implemented for [`IntoRust`] (closure travels foreign → Rust; the
/// invoke contract is an [`ImportedCallable<S>`](crate::ImportedCallable))
/// and for [`OutOfRust`] (closure travels Rust → foreign as a callback
/// parameter; the invoke contract is an
/// [`ExportedCallable<S>`](crate::ExportedCallable)). Each direction
/// supplies the AST-to-IR construction for its own closure shape, so
/// the param-lowering walk never holds a direction-agnostic
/// [`ClosureParam`](crate::ClosureParam) value.
pub(crate) trait ClosureParamSlot<S: SurfaceLower>: ParamDirection<S> + Sized {
    fn lower_closure_param(
        idx: &Index<'_>,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        name: CanonicalName,
        meta: ElementMeta,
        closure: &ClosureType,
    ) -> Result<ParamDecl<S, Self>, LowerError>;
}

impl<S: SurfaceLower> ClosureParamSlot<S> for IntoRust {
    fn lower_closure_param(
        idx: &Index<'_>,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        name: CanonicalName,
        meta: ElementMeta,
        closure: &ClosureType,
    ) -> Result<ParamDecl<S, IntoRust>, LowerError> {
        let param = super::lower_closure_param_into_rust::<S>(idx, ids, allocator, closure)?;
        Ok(<ParamDecl<S, IntoRust>>::closure(name, meta, param))
    }
}

impl<S: SurfaceLower> ClosureParamSlot<S> for OutOfRust {
    fn lower_closure_param(
        idx: &Index<'_>,
        ids: &DeclarationIds,
        allocator: &mut SymbolAllocator,
        name: CanonicalName,
        meta: ElementMeta,
        closure: &ClosureType,
    ) -> Result<ParamDecl<S, OutOfRust>, LowerError> {
        let param = super::lower_closure_param_out_of_rust::<S>(idx, ids, allocator, closure)?;
        Ok(<ParamDecl<S, OutOfRust>>::closure(name, meta, param))
    }
}

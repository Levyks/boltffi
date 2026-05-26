//! Lowers AST callables (methods, initializers, free functions) into
//! [`CallableDecl<S>`].
//!
//! Each axis of the IR's call shape — receiver mode, parameter
//! crossings, return crossing, error transport, execution kind — is
//! decided here from the source [`MethodDef`] (and friends) and the
//! surrounding [`CallableOwner`] context. The resulting
//! [`CallableDecl`] carries every decision a renderer needs without
//! re-running the dispatch.
//!
//! What this module supports today (intentionally narrow):
//!
//! - synchronous callables;
//! - by-value, by-ref, and by-mut-ref receivers;
//! - parameter and return types that lower through the existing
//!   [`super::types`] and [`super::codecs`] passes.
//!
//! What it rejects with a precise error (each will become a follow-up):
//!
//! - `async fn` ([`UnsupportedType::AsyncCallable`]);
//! - `impl Trait` and `Box<dyn Trait>` parameters
//!   ([`UnsupportedType::ImplTraitParameter`],
//!   [`UnsupportedType::BoxedDynParameter`]);
//! - parameters whose type references a declaration family the pass
//!   has not yet lowered — those are caught upstream by
//!   [`super::reject_unsupported`] so they cannot reach here.

mod params;
mod returns;

use boltffi_ast::{ExecutionKind, MethodDef, Receiver};

use crate::{CallableDecl, ExecutionDecl, Receive};

use super::{
    LowerError, error::UnsupportedType, ids::DeclarationIds, index::Index, surface::SurfaceLower,
};

/// Names the declaration that owns a callable.
///
/// Used to resolve `Self` inside parameter and return types and to
/// drive the symbol-naming convention. Carries a borrow into the
/// source AST so the lowering pass does not duplicate identity.
#[derive(Clone, Copy)]
pub(super) enum CallableOwner<'src> {
    /// Owned by a record.
    Record(&'src boltffi_ast::RecordDef),
    /// Owned by an enum.
    Enum(&'src boltffi_ast::EnumDef),
    /// Owned by a class.
    Class(&'src boltffi_ast::ClassDef),
}

impl<'src> CallableOwner<'src> {
    fn self_type_expr(self) -> boltffi_ast::TypeExpr {
        match self {
            Self::Record(record) => boltffi_ast::TypeExpr::Record(record.id.clone()),
            Self::Enum(enumeration) => boltffi_ast::TypeExpr::Enum(enumeration.id.clone()),
            Self::Class(class) => boltffi_ast::TypeExpr::Class(class.id.clone()),
        }
    }

    pub(super) fn owns_type_expr(self, type_expr: &boltffi_ast::TypeExpr) -> bool {
        match (self, type_expr) {
            (_, boltffi_ast::TypeExpr::SelfType) => true,
            (Self::Record(record), boltffi_ast::TypeExpr::Record(id)) => id == &record.id,
            (Self::Enum(enumeration), boltffi_ast::TypeExpr::Enum(id)) => id == &enumeration.id,
            (Self::Class(class), boltffi_ast::TypeExpr::Class(id)) => id == &class.id,
            _ => false,
        }
    }
}

/// Lowers one [`MethodDef`] into a [`CallableDecl<S>`] usable by both
/// regular methods and initializers.
///
/// The owner context resolves `Self`. The receiver follows the source.
/// Async and callback-shaped parameters are rejected with a specific
/// [`UnsupportedType`] so the gap is visible to the caller.
pub(super) fn lower_method<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    owner: CallableOwner<'_>,
    method: &MethodDef,
) -> Result<CallableDecl<S>, LowerError> {
    if matches!(method.execution, ExecutionKind::Async) {
        return Err(LowerError::unsupported_type(UnsupportedType::AsyncCallable));
    }

    let receiver = lower_receiver(method.receiver);
    let parameters = params::lower::<S>(idx, ids, owner, &method.parameters)?;
    let (returns, error) = returns::lower::<S>(idx, ids, owner, &method.returns)?;

    Ok(CallableDecl::new(
        receiver,
        parameters,
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?)
}

fn lower_receiver(receiver: Receiver) -> Option<Receive> {
    match receiver {
        Receiver::None => None,
        Receiver::Shared => Some(Receive::ByRef),
        Receiver::Mutable => Some(Receive::ByMutRef),
        Receiver::Owned => Some(Receive::ByValue),
    }
}

/// Substitutes occurrences of [`TypeExpr::SelfType`] with the owner's
/// concrete type expression.
///
/// Walks the expression once. Other `Self`-shaped sub-expressions
/// (`Vec<Self>`, `Option<Self>`, tuple elements, map keys/values,
/// closure parameters and returns, optional/sequence inner) all
/// recurse so a method like `fn neighbours(&self) -> Vec<Self>`
/// resolves correctly.
pub(super) fn substitute_self_type(
    owner: CallableOwner<'_>,
    type_expr: &boltffi_ast::TypeExpr,
) -> boltffi_ast::TypeExpr {
    use boltffi_ast::TypeExpr;
    match type_expr {
        TypeExpr::SelfType => owner.self_type_expr(),
        TypeExpr::Vec(inner) => TypeExpr::Vec(Box::new(substitute_self_type(owner, inner))),
        TypeExpr::Option(inner) => TypeExpr::Option(Box::new(substitute_self_type(owner, inner))),
        TypeExpr::Tuple(elements) => TypeExpr::Tuple(
            elements
                .iter()
                .map(|element| substitute_self_type(owner, element))
                .collect(),
        ),
        TypeExpr::Map { key, value } => TypeExpr::Map {
            key: Box::new(substitute_self_type(owner, key)),
            value: Box::new(substitute_self_type(owner, value)),
        },
        TypeExpr::Result { ok, err } => TypeExpr::Result {
            ok: Box::new(substitute_self_type(owner, ok)),
            err: Box::new(substitute_self_type(owner, err)),
        },
        TypeExpr::Closure(closure) => {
            let mut closure = (**closure).clone();
            closure.parameters = closure
                .parameters
                .iter()
                .map(|parameter| substitute_self_type(owner, parameter))
                .collect();
            closure.returns = match closure.returns {
                boltffi_ast::ReturnDef::Void => boltffi_ast::ReturnDef::Void,
                boltffi_ast::ReturnDef::Value(value) => {
                    boltffi_ast::ReturnDef::Value(substitute_self_type(owner, &value))
                }
            };
            TypeExpr::Closure(Box::new(closure))
        }
        TypeExpr::Primitive(_)
        | TypeExpr::String
        | TypeExpr::Bytes
        | TypeExpr::Record(_)
        | TypeExpr::Enum(_)
        | TypeExpr::Class(_)
        | TypeExpr::Callback(_)
        | TypeExpr::Custom(_)
        | TypeExpr::Parameter(_) => type_expr.clone(),
    }
}

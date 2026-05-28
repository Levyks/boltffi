//! Lowers AST callables (methods, initializers, free functions) into
//! the [`CallableDecl<S, K>`] family.
//!
//! Each axis of the IR's call shape (receiver mode, parameter
//! crossings, return crossing, error transport, execution kind) is
//! decided here from the source [`MethodDef`] (and friends) and the
//! surrounding [`CallableOwner`] context. The resulting
//! [`CallableDecl`] carries every decision a renderer needs without
//! re-running the dispatch.
//!
//! Scope splits along which side implements the body:
//!
//! - [`lower_exported_method`] and [`lower_function`] produce
//!   [`ExportedCallable<S>`] (`K = RustBody`): Rust implements, foreign
//!   calls in. They take a [`SymbolAllocator`] and the start callable's
//!   symbol name because async exported callables mint the lifecycle
//!   symbols on [`Surface::AsyncProtocol`] from that prefix.
//! - [`lower_imported_method`] produces an [`ImportedCallable<S>`]
//!   used by callback-trait dispatch (`K = ForeignBody`): foreign
//!   implements, Rust calls out. Imported async callables carry the
//!   surface protocol used by callback-trait dispatch.
//! - [`lower_closure_param_into_rust`] produces a closure parameter
//!   whose invoke contract is an [`ImportedCallable<S>`]. Closure
//!   signatures have no execution axis in the AST, so the invoke is
//!   always synchronous.
//!
//! What this module supports today:
//!
//! - synchronous and async exported callables, the latter through the
//!   surface's [`Surface::AsyncProtocol`] value built by
//!   [`super::async_protocol`];
//! - by-value, by-ref, and by-mut-ref receivers;
//! - callback-trait params and returns in all four shapes
//!   ([`TraitUseForm`](boltffi_ast::TraitUseForm) crossed with
//!   [`HandlePresence`](boltffi_ast::HandlePresence)), routed as
//!   nullable or required callback handles;
//! - `Result<(), E>` returns, which produce a void plan plus an
//!   encoded error channel;
//! - parameter and return types that lower through the existing
//!   [`super::types`] and [`super::codecs`] passes.
//!
//! What it rejects with a precise error (each is a follow-up gap):
//!
//! - the unit type `()` outside the `Result<(), E>` success channel
//!   ([`UnsupportedType::UnitInValuePosition`]);
//! - `Self` referenced from a callback-trait method signature
//!   ([`UnsupportedType::SelfInCallbackTrait`]);
//! - parameters whose type references a declaration family the pass
//!   has not yet lowered. Those are caught upstream by
//!   [`super::reject_unsupported`] so they cannot reach here.
//!
//! [`ExportedCallable<S>`]: crate::ExportedCallable
//! [`ImportedCallable<S>`]: crate::ImportedCallable
//! [`Surface::AsyncProtocol`]: crate::Surface::AsyncProtocol

mod params;
mod returns;

use boltffi_ast::{
    CanonicalName as SourceName, ClosureType, ExecutionKind, FunctionDef, MethodDef, ParameterDef,
    Receiver,
};

use crate::{
    ClosureForm, ClosureParameter, ClosureRegistration, ClosureReturn, Direction, ExecutionDecl,
    ExportedCallable, ForeignBody, ImportedCallable, IntoRust, OutOfRust, Receive, RustBody,
};

use super::{
    LowerError, error::UnsupportedType, ids::DeclarationIds, index::Index, surface::SurfaceLower,
    symbol::SymbolAllocator,
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
    /// Owned by a trait.
    Trait(&'src boltffi_ast::TraitDef),
    /// A top-level free function. Free functions have no `Self` and no
    /// owning type, so type-expression substitution rejects any `Self`
    /// reference encountered in this position.
    Function,
}

impl<'src> CallableOwner<'src> {
    fn self_type_expr(self) -> Result<boltffi_ast::TypeExpr, LowerError> {
        match self {
            Self::Record(record) => Ok(boltffi_ast::TypeExpr::Record(record.id.clone())),
            Self::Enum(enumeration) => Ok(boltffi_ast::TypeExpr::Enum(enumeration.id.clone())),
            Self::Class(class) => Ok(boltffi_ast::TypeExpr::Class {
                id: class.id.clone(),
                presence: boltffi_ast::HandlePresence::Required,
            }),
            Self::Trait(_) => Err(LowerError::unsupported_type(
                UnsupportedType::SelfInCallbackTrait,
            )),
            Self::Function => Err(LowerError::unsupported_type(UnsupportedType::SelfType)),
        }
    }

    pub(super) fn owns_type_expr(self, type_expr: &boltffi_ast::TypeExpr) -> bool {
        match (self, type_expr) {
            (Self::Trait(_) | Self::Function, boltffi_ast::TypeExpr::SelfType) => false,
            (_, boltffi_ast::TypeExpr::SelfType) => true,
            (Self::Record(record), boltffi_ast::TypeExpr::Record(id)) => id == &record.id,
            (Self::Enum(enumeration), boltffi_ast::TypeExpr::Enum(id)) => id == &enumeration.id,
            (
                Self::Class(class),
                boltffi_ast::TypeExpr::Class {
                    id,
                    presence: boltffi_ast::HandlePresence::Required,
                },
            ) => id == &class.id,
            (Self::Trait(source_trait), boltffi_ast::TypeExpr::Trait { id, .. }) => {
                id == &source_trait.id
            }
            _ => false,
        }
    }
}

/// Lowers a Rust-implemented [`MethodDef`] into an
/// [`ExportedCallable<S>`].
///
/// `start_symbol_name` is the symbol foreign code links against to
/// invoke this callable. For sync methods it is the only symbol the
/// callable references; for async methods it is the prefix used to
/// mint the lifecycle symbols on [`Surface::AsyncProtocol`]. The
/// allocator hands out fresh ids for each lifecycle symbol.
///
/// The owner context resolves `Self` inside parameter and return type
/// expressions. The receiver follows the source.
pub(super) fn lower_exported_method<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner<'_>,
    method: &MethodDef,
    start_symbol_name: &str,
) -> Result<ExportedCallable<S>, LowerError> {
    let receiver = lower_receiver(method.receiver);
    let parameters = params::lower::<S, IntoRust>(idx, ids, allocator, owner, &method.parameters)?;
    let (returns, error) = returns::lower::<S, _>(idx, ids, allocator, owner, &method.returns)?;
    let execution = lower_execution::<S>(allocator, method.execution, start_symbol_name)?;

    Ok(ExportedCallable::<S>::new(
        receiver, parameters, returns, error, execution,
    )?)
}

/// Lowers a foreign-implemented callback trait [`MethodDef`] into an
/// [`ImportedCallable<S>`].
///
/// Callback methods cross in the opposite direction from exported
/// methods: Rust pushes arguments out and reads the return back in.
/// Their dispatch target is not a [`NativeSymbol`](crate::NativeSymbol)
/// but a per-surface slot ([`crate::VTableSlot`] on native, an
pub(super) fn lower_imported_method<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    owner: CallableOwner<'_>,
    method: &MethodDef,
    execution: ExecutionDecl<S>,
) -> Result<ImportedCallable<S>, LowerError> {
    let receiver = lower_receiver(method.receiver);
    let parameters = params::lower::<S, OutOfRust>(idx, ids, allocator, owner, &method.parameters)?;
    let (returns, error) =
        returns::lower::<S, IntoRust>(idx, ids, allocator, owner, &method.returns)?;

    Ok(ImportedCallable::<S>::new(
        receiver, parameters, returns, error, execution,
    )?)
}

/// Lowers an inline [`ClosureType`] crossing into Rust as a parameter.
///
/// The closure was created on the foreign side, so its body lives
/// there. Rust holds the handle and invokes it. The invoke contract is
/// an [`ImportedCallable<S>`] (`K = ForeignBody`): args flow
/// [`OutOfRust`] at invocation, returns and error flow back as
/// [`IntoRust`]. The registration uses [`Receive::ByValue`] and the
/// surface's closure-registration shape.
///
/// Closure parameters have no source names, so the lowering pass
/// synthesises `arg0`, `arg1`, ... and reuses the regular parameter
/// machinery against them. Closure signatures have no execution axis in
/// the AST, so the invoke is always synchronous. `Self` references
/// reach the function-scoped substitution path and are rejected there.
pub(super) fn lower_closure_param_into_rust<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureParameter<S, IntoRust>, LowerError> {
    let parts = lower_closure_into_rust_parts(idx, ids, allocator, closure)?;
    Ok(ClosureParameter::new(
        parts.form,
        parts.registration,
        parts.invoke,
    ))
}

pub(super) fn lower_closure_return_into_rust<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureReturn<S, IntoRust>, LowerError> {
    let parts = lower_closure_into_rust_parts(idx, ids, allocator, closure)?;
    Ok(ClosureReturn::new(
        parts.form,
        parts.registration,
        parts.invoke,
    ))
}

struct ClosureIntoRustParts<S: crate::Surface> {
    form: ClosureForm,
    registration: ClosureRegistration<S, IntoRust>,
    invoke: ImportedCallable<S>,
}

fn lower_closure_into_rust_parts<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureIntoRustParts<S>, LowerError> {
    let (parameters, returns, error) =
        lower_closure_invoke_parts::<S, ForeignBody>(idx, ids, allocator, closure)?;
    let invoke = ImportedCallable::<S>::new(
        None,
        parameters,
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?;
    let registration = ClosureRegistration::<S, IntoRust>::new(
        S::incoming_closure_registration(closure)?,
        Receive::ByValue,
    );
    Ok(ClosureIntoRustParts {
        form: ClosureForm::from(closure.kind),
        registration,
        invoke,
    })
}

/// Lowers an inline [`ClosureType`] crossing out of Rust as a callback
/// parameter.
///
/// The closure was created on the Rust side and crosses out so foreign
/// code can invoke it from a foreign-body callable (callback method).
/// The body lives on the Rust side, so the invoke contract is an
/// [`ExportedCallable<S>`] (`K = RustBody`): args flow [`IntoRust`] at
/// invocation, returns and error flow back as [`OutOfRust`]. The
/// registration carries the surface's closure-registration shape with a
/// `()` receive slot.
pub(super) fn lower_closure_param_out_of_rust<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureParameter<S, OutOfRust>, LowerError> {
    let parts = lower_closure_out_of_rust_parts(idx, ids, allocator, closure)?;
    Ok(ClosureParameter::new(
        parts.form,
        parts.registration,
        parts.invoke,
    ))
}

pub(super) fn lower_closure_return_out_of_rust<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureReturn<S, OutOfRust>, LowerError> {
    let parts = lower_closure_out_of_rust_parts(idx, ids, allocator, closure)?;
    Ok(ClosureReturn::new(
        parts.form,
        parts.registration,
        parts.invoke,
    ))
}

struct ClosureOutOfRustParts<S: crate::Surface> {
    form: ClosureForm,
    registration: ClosureRegistration<S, OutOfRust>,
    invoke: ExportedCallable<S>,
}

fn lower_closure_out_of_rust_parts<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureOutOfRustParts<S>, LowerError> {
    let (parameters, returns, error) =
        lower_closure_invoke_parts::<S, RustBody>(idx, ids, allocator, closure)?;
    let invoke = ExportedCallable::<S>::new(
        None,
        parameters,
        returns,
        error,
        ExecutionDecl::synchronous(),
    )?;
    let shape = S::outgoing_closure_registration(allocator, closure)?;
    #[allow(clippy::let_unit_value)]
    let receive = <OutOfRust as Direction>::receive_from(Receive::ByValue);
    let registration = ClosureRegistration::<S, OutOfRust>::new(shape, receive);
    Ok(ClosureOutOfRustParts {
        form: ClosureForm::from(closure.kind),
        registration,
        invoke,
    })
}

type ClosureInvokeParts<S, K> = (
    Vec<crate::ParamDecl<S, <K as crate::CallableScope>::ParamDirection>>,
    crate::ReturnDecl<S, <K as crate::CallableScope>::ReturnDirection>,
    crate::ErrorDecl<S, <K as crate::CallableScope>::ReturnDirection>,
);

/// Shared closure-invoke argument lowering used by both directions.
///
/// Synthesises positional names for the parameter list and routes both
/// the parameters and the return through the scope `K`'s directions, so
/// the invoke's params and returns flow correctly for the closure's
/// body location.
fn lower_closure_invoke_parts<S: SurfaceLower, K: crate::CallableScope>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    closure: &ClosureType,
) -> Result<ClosureInvokeParts<S, K>, LowerError>
where
    K::ParamDirection: params::LowerClosure<S>,
    K::ReturnDirection: params::LowerClosure<S>,
{
    let owner = CallableOwner::Function;
    let parameters = closure
        .parameters
        .iter()
        .enumerate()
        .map(|(index, type_expr)| {
            ParameterDef::value(SourceName::single(format!("arg{index}")), type_expr.clone())
        })
        .collect::<Vec<_>>();
    let lowered_params =
        params::lower::<S, K::ParamDirection>(idx, ids, allocator, owner, &parameters)?;
    let (returns, error) =
        returns::lower::<S, K::ReturnDirection>(idx, ids, allocator, owner, &closure.returns)?;
    Ok((lowered_params, returns, error))
}

/// Lowers one [`FunctionDef`] into an [`ExportedCallable<S>`].
///
/// Free functions have no receiver and no `Self`; the owner context is
/// [`CallableOwner::Function`], which rejects any `Self` reference
/// found while walking parameter and return types. Async free
/// functions lower through the same lifecycle protocol as async
/// methods; `start_symbol_name` names the start symbol foreign code
/// links to invoke the operation, and the lifecycle symbols are minted
/// with that name as prefix.
pub(super) fn lower_function<S: SurfaceLower>(
    idx: &Index<'_>,
    ids: &DeclarationIds,
    allocator: &mut SymbolAllocator,
    function: &FunctionDef,
    start_symbol_name: &str,
) -> Result<ExportedCallable<S>, LowerError> {
    let owner = CallableOwner::Function;
    let parameters =
        params::lower::<S, IntoRust>(idx, ids, allocator, owner, &function.parameters)?;
    let (returns, error) = returns::lower::<S, _>(idx, ids, allocator, owner, &function.returns)?;
    let execution = lower_execution::<S>(allocator, function.execution, start_symbol_name)?;

    Ok(ExportedCallable::<S>::new(
        None, parameters, returns, error, execution,
    )?)
}

/// Builds the [`ExecutionDecl<S>`] for an exported callable.
///
/// Sync callables yield [`ExecutionDecl::synchronous`] without touching
/// the allocator. Async callables defer to the surface's
/// [`AsyncProtocolBuilder`] to mint the lifecycle symbols from the
/// start symbol prefix and wrap them in
/// [`ExecutionDecl::asynchronous`].
fn lower_execution<S: SurfaceLower>(
    allocator: &mut SymbolAllocator,
    execution: ExecutionKind,
    start_symbol_name: &str,
) -> Result<ExecutionDecl<S>, LowerError> {
    match execution {
        ExecutionKind::Sync => Ok(ExecutionDecl::synchronous()),
        ExecutionKind::Async => {
            let protocol = S::build_protocol(allocator, start_symbol_name)?;
            Ok(ExecutionDecl::asynchronous(protocol))
        }
    }
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
) -> Result<boltffi_ast::TypeExpr, LowerError> {
    use boltffi_ast::TypeExpr;
    Ok(match type_expr {
        TypeExpr::SelfType => owner.self_type_expr()?,
        TypeExpr::Vec(inner) => TypeExpr::Vec(Box::new(substitute_self_type(owner, inner)?)),
        TypeExpr::Option(inner) => match (owner, inner.as_ref()) {
            (CallableOwner::Class(class), TypeExpr::SelfType) => TypeExpr::Class {
                id: class.id.clone(),
                presence: boltffi_ast::HandlePresence::Nullable,
            },
            _ => TypeExpr::Option(Box::new(substitute_self_type(owner, inner)?)),
        },
        TypeExpr::Tuple(elements) => TypeExpr::Tuple(
            elements
                .iter()
                .map(|element| substitute_self_type(owner, element))
                .collect::<Result<Vec<_>, LowerError>>()?,
        ),
        TypeExpr::Map { key, value } => TypeExpr::Map {
            key: Box::new(substitute_self_type(owner, key)?),
            value: Box::new(substitute_self_type(owner, value)?),
        },
        TypeExpr::Result { ok, err } => TypeExpr::Result {
            ok: Box::new(substitute_self_type(owner, ok)?),
            err: Box::new(substitute_self_type(owner, err)?),
        },
        TypeExpr::Closure(closure) => {
            let mut closure = (**closure).clone();
            closure.parameters = closure
                .parameters
                .iter()
                .map(|parameter| substitute_self_type(owner, parameter))
                .collect::<Result<Vec<_>, LowerError>>()?;
            closure.returns = match closure.returns {
                boltffi_ast::ReturnDef::Void => boltffi_ast::ReturnDef::Void,
                boltffi_ast::ReturnDef::Value(value) => {
                    boltffi_ast::ReturnDef::Value(substitute_self_type(owner, &value)?)
                }
            };
            TypeExpr::Closure(Box::new(closure))
        }
        TypeExpr::Primitive(_)
        | TypeExpr::Unit
        | TypeExpr::String
        | TypeExpr::Bytes
        | TypeExpr::Record(_)
        | TypeExpr::Enum(_)
        | TypeExpr::Class { .. }
        | TypeExpr::Trait { .. }
        | TypeExpr::Custom(_)
        | TypeExpr::Parameter(_) => type_expr.clone(),
    })
}

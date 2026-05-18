use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::{
    AsyncProtocolIntrospect, BindingError, BindingErrorKind, BufferShapeRules, CanonicalName,
    ElementMeta, HandleTarget, IntegerRepr, NativeSymbol, ReadPlan, Surface, TypeRef, WritePlan,
};

/// One call shape ready to be turned into target code.
///
/// Carries every decision about how the call crosses the boundary: the
/// receiver mode when the callable is a method, how each argument
/// enters Rust, how the result leaves, how errors are reported, and
/// whether the call is synchronous or asynchronous. The call site
/// (linker symbol or vtable slot) lives on the owning declaration, not
/// on the callable.
///
/// The receiver's crossing form belongs to the owning declaration. A
/// method on a class crosses its receiver as a handle; a method on a
/// direct record crosses by direct memory; a method on a data enum
/// crosses encoded. The callable only records whether `self` is owned,
/// shared, or mutably borrowed. Renderers must render every method
/// alongside its owner so the crossing form is available.
///
/// Generic over `S: Surface` so target-divergent shapes (buffer
/// layouts, handle carriers, async protocol) are picked once at
/// classification and never re-derived by consumers.
///
/// # Example
///
/// `fn add(a: i32, b: i32) -> i32` produces a `CallableDecl<S>` with no
/// receiver, two direct-scalar parameters, a direct-scalar return, no
/// error transport, and synchronous execution. Its native symbol lives
/// on the surrounding [`crate::FunctionDecl`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct CallableDecl<S: Surface> {
    receiver: Option<Receive>,
    params: Vec<ParamDecl<S>>,
    returns: ReturnDecl<S>,
    error: ErrorDecl<S>,
    execution: ExecutionDecl<S>,
}

impl<S: Surface> CallableDecl<S> {
    pub(crate) fn new(
        receiver: Option<Receive>,
        params: Vec<ParamDecl<S>>,
        returns: ReturnDecl<S>,
        error: ErrorDecl<S>,
        execution: ExecutionDecl<S>,
    ) -> Result<Self, BindingError> {
        let callable = Self {
            receiver,
            params,
            returns,
            error,
            execution,
        };
        callable.validate()?;
        Ok(callable)
    }

    /// Returns `Ok` when the callable's return shape, error channel, and
    /// per-param buffer shapes form a coherent native signature.
    ///
    /// Rejects:
    ///
    /// - Return and error both claiming the native return slot.
    /// - A buffer shape on a parameter encoded crossing that the
    ///   surface forbids (e.g. `wasm32::BufferShape::Packed`).
    /// - A buffer shape on a return or error encoded crossing that the
    ///   surface forbids (e.g. any `Slice`, since a borrowed view
    ///   cannot leave Rust with no owner to free it).
    ///
    /// Re-runs at the contract boundary so values reconstructed through
    /// `Deserialize` cannot bypass the invariants enforced by
    /// [`Self::new`].
    pub fn validate(&self) -> Result<(), BindingError> {
        if self.returns.lift().uses_return_slot() && self.error.uses_return_slot() {
            return Err(BindingError::new(BindingErrorKind::ReturnSlotConflict));
        }
        for param in &self.params {
            if let Some(shape) = param.lower().buffer_shape()
                && !shape.is_valid_in_param()
            {
                return Err(BindingError::new(BindingErrorKind::PackedInParamPosition));
            }
        }
        if let Some(shape) = self.returns.lift().buffer_shape()
            && !shape.is_valid_in_return()
        {
            return Err(BindingError::new(BindingErrorKind::SliceInReturnPosition));
        }
        if let Some(shape) = self.error.buffer_shape()
            && !shape.is_valid_in_return()
        {
            return Err(BindingError::new(BindingErrorKind::SliceInReturnPosition));
        }
        Ok(())
    }

    /// Returns the receiver mode.
    ///
    /// `None` for free functions and static methods. `Some` for instance
    /// methods, where the [`Receive`] variant names whether `self` is
    /// taken by value, by shared reference, or by mutable reference.
    pub const fn receiver(&self) -> Option<Receive> {
        self.receiver
    }

    /// Returns the parameters in call order.
    pub fn params(&self) -> &[ParamDecl<S>] {
        &self.params
    }

    /// Returns the return shape.
    pub fn returns(&self) -> &ReturnDecl<S> {
        &self.returns
    }

    /// Returns the error transport.
    pub fn error(&self) -> &ErrorDecl<S> {
        &self.error
    }

    /// Returns the execution mode.
    pub fn execution(&self) -> &ExecutionDecl<S> {
        &self.execution
    }

    /// Iterates over every native symbol this callable references.
    ///
    /// A synchronous callable yields nothing; an asynchronous callable
    /// yields the async protocol's lifecycle symbols.
    pub fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        match &self.execution {
            ExecutionDecl::Synchronous(_) => Box::new(std::iter::empty()),
            ExecutionDecl::Asynchronous(protocol) => protocol.native_symbols(),
        }
    }
}

/// One parameter accepted by a callable.
///
/// Carries the canonical name the source wrote, per-element metadata
/// for documentation and defaults, and the [`LowerPlan`] that names
/// every decision about how the value crosses. The crossing decision
/// lives entirely on the lower plan; nothing on this struct lets a
/// consumer second-guess it.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned"
))]
pub struct ParamDecl<S: Surface> {
    name: CanonicalName,
    meta: ElementMeta,
    lower: LowerPlan<S>,
}

impl<S: Surface> ParamDecl<S> {
    pub(crate) fn new(name: CanonicalName, meta: ElementMeta, lower: LowerPlan<S>) -> Self {
        Self { name, meta, lower }
    }

    /// Returns the parameter name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    /// Returns the lower plan.
    pub fn lower(&self) -> &LowerPlan<S> {
        &self.lower
    }
}

/// How a foreign-language argument enters Rust at call time.
///
/// The variant tag is the crossing decision: `Direct` passes the value
/// through a native call slot as itself, `Encoded` moves it through a
/// wire buffer, `Handle` carries an opaque token into a Rust-owned
/// resource. Each variant additionally records the Rust-side [`Receive`]
/// mode the inner function uses.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum LowerPlan<S: Surface> {
    /// Value crosses through a native call slot as itself.
    Direct {
        /// Spelling type for the foreign side.
        ty: TypeRef,
        /// Rust-side receive mode.
        receive: Receive,
    },
    /// Value crosses as encoded bytes through a wire buffer.
    Encoded {
        /// Spelling type for the foreign side.
        ty: TypeRef,
        /// Plan used to write the value into wire bytes.
        write: WritePlan,
        /// Slot layout the encoded bytes use to cross.
        shape: S::BufferShape,
        /// Rust-side receive mode.
        receive: Receive,
    },
    /// Value crosses as an opaque handle.
    Handle {
        /// Declaration the handle stands in for.
        target: HandleTarget,
        /// Carrier used to move the handle across the boundary.
        carrier: S::HandleCarrier,
        /// Rust-side receive mode.
        receive: Receive,
    },
}

impl<S: Surface> LowerPlan<S> {
    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::Encoded { shape, .. } => Some(*shape),
            _ => None,
        }
    }
}

/// The result a callable produces.
///
/// Carries per-element metadata and the [`LiftPlan`] that says how the
/// value reaches foreign code, including its position in the native
/// signature. A void return is represented as [`LiftPlan::Void`] rather
/// than the absence of a `ReturnDecl`.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned"
))]
pub struct ReturnDecl<S: Surface> {
    meta: ElementMeta,
    lift: LiftPlan<S>,
}

impl<S: Surface> ReturnDecl<S> {
    pub(crate) fn new(meta: ElementMeta, lift: LiftPlan<S>) -> Self {
        Self { meta, lift }
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    /// Returns the lift plan.
    pub fn lift(&self) -> &LiftPlan<S> {
        &self.lift
    }
}

/// How a Rust return value reaches foreign code.
///
/// Mirror of [`LowerPlan`] for the return direction. The variant tag
/// carries both the crossing form and the position in the native
/// signature: the bare variants use the native return slot; the
/// variants suffixed `Out` write through a synthetic trailing
/// out-pointer parameter while the return slot is reserved for the
/// error channel.
///
/// # Example
///
/// `fn translate(...) -> Point` (where `Point` is a direct record)
/// lifts as `LiftPlan::Direct { ty: TypeRef::Record(point_id) }` when
/// the call is infallible. Paired with a [`ErrorDecl::StatusReturn`] it
/// lifts as `LiftPlan::DirectOut { ... }` so the status integer can
/// occupy the native return slot.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum LiftPlan<S: Surface> {
    /// The callable returns no value.
    Void,
    /// Direct value occupies the native return slot.
    Direct {
        /// Spelling type for the foreign side.
        ty: TypeRef,
    },
    /// Direct value written through a trailing out-pointer parameter.
    DirectOut {
        /// Spelling type for the foreign side.
        ty: TypeRef,
    },
    /// Encoded payload occupies the native return slot.
    Encoded {
        /// Spelling type for the foreign side.
        ty: TypeRef,
        /// Plan used to read the value back from wire bytes.
        read: ReadPlan,
        /// Slot layout the encoded bytes use to cross.
        shape: S::BufferShape,
    },
    /// Encoded payload written through a trailing out-pointer parameter.
    EncodedOut {
        /// Spelling type for the foreign side.
        ty: TypeRef,
        /// Plan used to read the value back from wire bytes.
        read: ReadPlan,
        /// Slot layout the encoded bytes use to cross.
        shape: S::BufferShape,
    },
    /// Opaque handle in the native return slot.
    Handle {
        /// Declaration the handle stands in for.
        target: HandleTarget,
        /// Carrier used to move the handle across the boundary.
        carrier: S::HandleCarrier,
    },
    /// Opaque handle written through a trailing out-pointer parameter.
    HandleOut {
        /// Declaration the handle stands in for.
        target: HandleTarget,
        /// Carrier used to move the handle across the boundary.
        carrier: S::HandleCarrier,
    },
}

impl<S: Surface> LiftPlan<S> {
    pub(crate) fn into_out(self) -> Self {
        match self {
            Self::Direct { ty } => Self::DirectOut { ty },
            Self::Encoded { ty, read, shape } => Self::EncodedOut { ty, read, shape },
            Self::Handle { target, carrier } => Self::HandleOut { target, carrier },
            other => other,
        }
    }

    pub(crate) const fn uses_return_slot(&self) -> bool {
        matches!(
            self,
            Self::Direct { .. } | Self::Encoded { .. } | Self::Handle { .. }
        )
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::Encoded { shape, .. } | Self::EncodedOut { shape, .. } => Some(*shape),
            _ => None,
        }
    }
}

/// How a fallible callable reports its error to foreign code.
///
/// `None` means the callable cannot fail across the boundary. `Status`
/// variants carry an integer where one designated value is success and
/// the others map to specific failures. `Encoded` variants carry the
/// failure as a typed payload. The variant tag selects whether the
/// error occupies the native return slot or a trailing out-pointer
/// parameter, the same axis [`LiftPlan`] uses for the success value.
///
/// # Example
///
/// `fn parse(...) -> Result<Number, ParseError>` produces
/// `ErrorDecl::StatusReturn { repr: IntegerRepr::I32 }` paired with
/// `LiftPlan::DirectOut`: the status code lives in the native return
/// slot, the parsed number is written through the out-pointer.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ErrorDecl<S: Surface> {
    /// Cannot fail across the boundary.
    None(#[serde(skip)] PhantomData<S>),
    /// Status integer in the native return slot.
    StatusReturn {
        /// Integer representation used for the status.
        repr: IntegerRepr,
    },
    /// Status integer through a trailing out-pointer parameter.
    StatusOut {
        /// Integer representation used for the status.
        repr: IntegerRepr,
    },
    /// Encoded error in the native return slot.
    EncodedReturn {
        /// Logical error type.
        ty: TypeRef,
        /// Plan used to read the error value.
        read: ReadPlan,
        /// Slot layout the encoded bytes use to cross.
        shape: S::BufferShape,
    },
    /// Encoded error through a trailing out-pointer parameter.
    EncodedOut {
        /// Logical error type.
        ty: TypeRef,
        /// Plan used to read the error value.
        read: ReadPlan,
        /// Slot layout the encoded bytes use to cross.
        shape: S::BufferShape,
    },
}

impl<S: Surface> ErrorDecl<S> {
    /// Returns an `ErrorDecl::None` value.
    pub fn none() -> Self {
        Self::None(PhantomData)
    }

    pub(crate) const fn uses_return_slot(&self) -> bool {
        matches!(self, Self::StatusReturn { .. } | Self::EncodedReturn { .. })
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::EncodedReturn { shape, .. } | Self::EncodedOut { shape, .. } => Some(*shape),
            _ => None,
        }
    }
}

/// Whether a callable returns immediately or through an async protocol.
///
/// `Synchronous` means control returns when the call returns.
/// `Asynchronous` carries the surface's chosen async protocol value
/// (poll handle on native, synchronous-poll on wasm, and so on).
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::AsyncProtocol: Serialize",
    deserialize = "S::AsyncProtocol: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ExecutionDecl<S: Surface> {
    /// Control returns when the call returns.
    Synchronous(#[serde(skip)] PhantomData<S>),
    /// Control returns through an async protocol.
    Asynchronous(S::AsyncProtocol),
}

impl<S: Surface> ExecutionDecl<S> {
    /// Returns the synchronous variant.
    pub fn synchronous() -> Self {
        Self::Synchronous(PhantomData)
    }
}

/// How the inner Rust function receives a parameter or receiver.
///
/// Names what the source wrote: `ByValue` for `T`, `ByRef` for `&T`,
/// `ByMutRef` for `&mut T`. The native call slot does not change shape
/// based on this value; the extern wrapper reconciles ownership when
/// invoking the inner Rust function. Generated host APIs may still
/// surface the distinction in the rendered language (Swift `inout`,
/// Kotlin receiver semantics for handles, and so on), so renderers are
/// free to consult it.
///
/// # Example
///
/// `fn area(rect: &Rectangle)` records its parameter as
/// `Receive::ByRef`. `fn finalize(self)` records its receiver as
/// `Receive::ByValue`.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Receive {
    /// `self` or by-value parameter. Rust takes ownership.
    ByValue,
    /// `&self` or `&T`.
    ByRef,
    /// `&mut self` or `&mut T`.
    ByMutRef,
}

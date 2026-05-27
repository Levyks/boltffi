use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use crate::{
    AsyncProtocolIntrospect, BindingError, BindingErrorKind, BufferShapeRules, CallableScope,
    CanonicalName, Direction, ElementMeta, ForeignBody, HandlePresence, HandleTarget, IntegerRepr,
    IntoRust, NativeSymbol, OutOfRust, Primitive, RustBody, Surface, TypeRef,
};

/// One call shape ready to be turned into target code.
///
/// Carries the receiver mode, the parameter crossings, the return
/// crossing, the error channel, and the execution kind. The call site
/// (linker symbol or vtable slot) lives on the owning declaration, not
/// on the callable.
///
/// `S` is the target surface. `K` is the body scope; its
/// `ParamDirection` flows into every parameter and its `ReturnDirection`
/// flows into the return and the error channel.
///
/// # Example
///
/// `fn add(a: i32, b: i32) -> i32` lowers to a
/// `CallableDecl<S, RustBody>` with no receiver, two
/// `ParamPlan::Direct` parameters, a `ReturnPlan::DirectViaReturnSlot`
/// return, `ErrorDecl::None`, and synchronous execution.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::AsyncProtocol: Serialize, S::ClosureRegistration: Serialize, K::ParamDirection: ParamDirection<S>, K::ReturnDirection: Direction, <K::ParamDirection as ParamDirection<S>>::Payload: Serialize, <K::ReturnDirection as Direction>::Codec: Serialize, <K::ReturnDirection as Direction>::Receive: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned, S::ClosureRegistration: serde::de::DeserializeOwned, K::ParamDirection: ParamDirection<S>, K::ReturnDirection: Direction, <K::ParamDirection as ParamDirection<S>>::Payload: serde::de::DeserializeOwned, <K::ReturnDirection as Direction>::Codec: serde::de::DeserializeOwned, <K::ReturnDirection as Direction>::Receive: serde::de::DeserializeOwned"
))]
pub struct CallableDecl<S: Surface, K: CallableScope>
where
    K::ParamDirection: ParamDirection<S>,
{
    receiver: Option<Receive>,
    params: Vec<ParamDecl<S, K::ParamDirection>>,
    returns: ReturnDecl<S, K::ReturnDirection>,
    error: ErrorDecl<S, K::ReturnDirection>,
    execution: ExecutionDecl<S>,
}

impl<S: Surface, K: CallableScope> CallableDecl<S, K>
where
    K::ParamDirection: ParamDirection<S>,
{
    pub(crate) fn new(
        receiver: Option<Receive>,
        params: Vec<ParamDecl<S, K::ParamDirection>>,
        returns: ReturnDecl<S, K::ReturnDirection>,
        error: ErrorDecl<S, K::ReturnDirection>,
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

    /// Checks the slot-conflict and buffer-shape invariants.
    ///
    /// Fails when:
    /// - both the return and the error channel use the native return
    ///   slot;
    /// - an encoded param has a buffer shape forbidden on params for
    ///   this surface (e.g. `wasm32::BufferShape::Packed`);
    /// - an encoded return or error has a buffer shape forbidden on
    ///   return slots (e.g. any `Slice`).
    ///
    /// `Bindings::validate` calls this on every callable.
    pub fn validate(&self) -> Result<(), BindingError> {
        if self.returns.plan().uses_return_slot() && self.error.uses_return_slot() {
            return Err(BindingError::new(BindingErrorKind::ReturnSlotConflict));
        }
        for param in &self.params {
            if let Some(shape) = param.buffer_shape()
                && !shape.is_valid_in_param()
            {
                return Err(BindingError::new(BindingErrorKind::PackedInParamPosition));
            }
        }
        if let Some(shape) = self.returns.plan().buffer_shape()
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

    /// Returns the receiver mode, or `None` for free functions and
    /// static methods.
    pub const fn receiver(&self) -> Option<Receive> {
        self.receiver
    }

    /// Returns the parameters in call order.
    pub fn params(&self) -> &[ParamDecl<S, K::ParamDirection>] {
        &self.params
    }

    /// Returns the return shape.
    pub fn returns(&self) -> &ReturnDecl<S, K::ReturnDirection> {
        &self.returns
    }

    /// Returns the error transport.
    pub fn error(&self) -> &ErrorDecl<S, K::ReturnDirection> {
        &self.error
    }

    /// Returns the execution mode.
    pub fn execution(&self) -> &ExecutionDecl<S> {
        &self.execution
    }

    /// Iterates the native symbols this callable references.
    ///
    /// Empty for synchronous callables. For async callables, yields the
    /// async protocol's lifecycle symbols.
    pub fn native_symbols(&self) -> Box<dyn Iterator<Item = &NativeSymbol> + '_> {
        match &self.execution {
            ExecutionDecl::Synchronous(_) => Box::new(std::iter::empty()),
            ExecutionDecl::Asynchronous(protocol) => protocol.native_symbols(),
        }
    }
}

/// A callable whose body is implemented in Rust. Foreign code calls
/// in. Used for free functions, record/enum/class methods, and
/// initializers.
pub type ExportedCallable<S> = CallableDecl<S, RustBody>;

/// A callable whose body is implemented in foreign code. Rust calls
/// out. Used for callback trait methods and inline closure
/// invocations.
pub type ImportedCallable<S> = CallableDecl<S, ForeignBody>;

/// Direction-specific payload carried by a parameter declaration.
pub trait ParamDirection<S: Surface>: Direction {
    /// Concrete payload shape admitted by this direction.
    type Payload: Clone + Debug + Eq + Hash + PartialEq + Serialize + for<'de> Deserialize<'de>;

    /// Wraps a value crossing as this direction's parameter payload.
    fn value_payload(plan: ParamPlan<S, Self>) -> Self::Payload;

    /// Returns the encoded buffer shape when the payload carries one.
    fn buffer_shape(payload: &Self::Payload) -> Option<S::BufferShape>;
}

/// One incoming parameter crossing.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::ClosureRegistration: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::ClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub enum IncomingParam<S: Surface> {
    /// One value crossing into Rust.
    Value(ParamPlan<S, IntoRust>),
    /// Inline closure callback crossing into Rust.
    Closure(ClosureParam<S>),
}

impl<S: Surface> IncomingParam<S> {
    pub(crate) fn as_value(&self) -> Option<&ParamPlan<S, IntoRust>> {
        match self {
            Self::Value(plan) => Some(plan),
            Self::Closure(_) => None,
        }
    }

    pub(crate) fn as_closure(&self) -> Option<&ClosureParam<S>> {
        match self {
            Self::Closure(closure) => Some(closure),
            Self::Value(_) => None,
        }
    }
}

/// One outgoing parameter crossing.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned"
))]
pub struct OutgoingParam<S: Surface> {
    plan: ParamPlan<S, OutOfRust>,
}

impl<S: Surface> OutgoingParam<S> {
    pub(crate) fn new(plan: ParamPlan<S, OutOfRust>) -> Self {
        Self { plan }
    }

    /// Returns the value crossing plan.
    pub fn plan(&self) -> &ParamPlan<S, OutOfRust> {
        &self.plan
    }
}

impl<S: Surface> ParamDirection<S> for IntoRust {
    type Payload = IncomingParam<S>;

    fn value_payload(plan: ParamPlan<S, Self>) -> Self::Payload {
        IncomingParam::Value(plan)
    }

    fn buffer_shape(payload: &Self::Payload) -> Option<S::BufferShape> {
        match payload {
            IncomingParam::Value(plan) => plan.buffer_shape(),
            IncomingParam::Closure(_) => None,
        }
    }
}

impl<S: Surface> ParamDirection<S> for OutOfRust {
    type Payload = OutgoingParam<S>;

    fn value_payload(plan: ParamPlan<S, Self>) -> Self::Payload {
        OutgoingParam::new(plan)
    }

    fn buffer_shape(payload: &Self::Payload) -> Option<S::BufferShape> {
        payload.plan().buffer_shape()
    }
}

/// One named parameter crossing.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "D: ParamDirection<S>, D::Payload: Serialize",
    deserialize = "D: ParamDirection<S>, D::Payload: serde::de::DeserializeOwned"
))]
pub struct ParamDecl<S: Surface, D: ParamDirection<S>> {
    name: CanonicalName,
    meta: ElementMeta,
    payload: D::Payload,
}

impl<S: Surface, D: ParamDirection<S>> ParamDecl<S, D> {
    /// Returns the parameter name.
    pub fn name(&self) -> &CanonicalName {
        &self.name
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    /// Returns the direction-specific payload.
    pub fn payload(&self) -> &D::Payload {
        &self.payload
    }

    pub(crate) fn value(name: CanonicalName, meta: ElementMeta, plan: ParamPlan<S, D>) -> Self {
        Self {
            name,
            meta,
            payload: D::value_payload(plan),
        }
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        D::buffer_shape(&self.payload)
    }
}

impl<S: Surface> ParamDecl<S, IntoRust> {
    pub(crate) fn as_value(&self) -> Option<&ParamPlan<S, IntoRust>> {
        self.payload.as_value()
    }

    pub(crate) fn as_closure(&self) -> Option<&ClosureParam<S>> {
        self.payload.as_closure()
    }

    pub(crate) fn closure(
        name: CanonicalName,
        meta: ElementMeta,
        closure: ClosureParam<S>,
    ) -> Self {
        Self {
            name,
            meta,
            payload: IncomingParam::Closure(closure),
        }
    }
}

impl<S: Surface> ParamDecl<S, OutOfRust> {
    pub(crate) fn as_value(&self) -> Option<&ParamPlan<S, OutOfRust>> {
        Some(self.payload.plan())
    }
}

/// An inline closure parameter and the contract for invoking it.
///
/// `form` records the source spelling (`fn(...)`, `impl Fn`,
/// `impl FnMut`, `impl FnOnce`). `registration` describes the handle
/// that crosses when the closure is passed across the boundary.
/// `invoke` is the call shape Rust uses on each invocation, with the
/// closure body sitting on the foreign side.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, S::ClosureRegistration: Serialize, S::AsyncProtocol: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, S::ClosureRegistration: serde::de::DeserializeOwned, S::AsyncProtocol: serde::de::DeserializeOwned"
))]
pub struct ClosureParam<S: Surface> {
    form: ClosureForm,
    registration: ClosureRegistration<S, IntoRust>,
    invoke: Box<ImportedCallable<S>>,
}

impl<S: Surface> ClosureParam<S> {
    pub(crate) fn new(
        form: ClosureForm,
        registration: ClosureRegistration<S, IntoRust>,
        invoke: ImportedCallable<S>,
    ) -> Self {
        Self {
            form,
            registration,
            invoke: Box::new(invoke),
        }
    }

    /// Returns the source spelling.
    pub fn form(&self) -> ClosureForm {
        self.form
    }

    /// Returns the handle registration.
    pub fn registration(&self) -> &ClosureRegistration<S, IntoRust> {
        &self.registration
    }

    /// Returns the invocation contract.
    pub fn invoke(&self) -> &ImportedCallable<S> {
        &self.invoke
    }
}

/// The source spelling of a closure parameter.
///
/// Every form crosses the wire the same way; renderers consult this
/// when emitting the Rust-side binding so the user-facing trait bound
/// (`Fn`, `FnMut`, `FnOnce`, or a bare function pointer) matches what
/// the user wrote.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ClosureForm {
    /// Bare `fn(...)` function-pointer parameter.
    FunctionPointer,
    /// `impl Fn(...)` parameter.
    Fn,
    /// `impl FnMut(...)` parameter.
    FnMut,
    /// `impl FnOnce(...)` parameter.
    FnOnce,
}

impl From<boltffi_ast::ClosureKind> for ClosureForm {
    fn from(kind: boltffi_ast::ClosureKind) -> Self {
        match kind {
            boltffi_ast::ClosureKind::FunctionPointer => Self::FunctionPointer,
            boltffi_ast::ClosureKind::Fn => Self::Fn,
            boltffi_ast::ClosureKind::FnMut => Self::FnMut,
            boltffi_ast::ClosureKind::FnOnce => Self::FnOnce,
        }
    }
}

/// The handle crossing for a closure parameter.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::ClosureRegistration: Serialize, D: Direction, D::Receive: Serialize",
    deserialize = "S::ClosureRegistration: serde::de::DeserializeOwned, D: Direction, D::Receive: serde::de::DeserializeOwned"
))]
pub struct ClosureRegistration<S: Surface, D: Direction> {
    shape: S::ClosureRegistration,
    receive: D::Receive,
}

impl<S: Surface, D: Direction> ClosureRegistration<S, D> {
    pub(crate) fn new(shape: S::ClosureRegistration, receive: D::Receive) -> Self {
        Self { shape, receive }
    }

    /// Returns the surface registration shape.
    pub fn shape(&self) -> &S::ClosureRegistration {
        &self.shape
    }

    /// Returns the receive mode of the registration slot.
    pub fn receive(&self) -> D::Receive {
        self.receive
    }
}

/// How one value crosses the boundary as a parameter slot in
/// direction `D`.
///
/// Each variant describes a distinct wire shape and is reachable
/// independently. `D::Codec` picks the foreign-side codec orientation
/// for encoded crossings, and `D::Receive` picks the Rust-side receive
/// mode for slots that have one.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, D: Direction, D::Codec: Serialize, D::Receive: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned, D::Receive: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ParamPlan<S: Surface, D: Direction> {
    /// Value occupies a native call slot directly.
    Direct {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Rust-side receive mode.
        receive: D::Receive,
    },
    /// Value crosses as encoded bytes.
    Encoded {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
        /// Rust-side receive mode.
        receive: D::Receive,
    },
    /// Value crosses as an opaque handle to a class or callback.
    Handle {
        /// Declaration the handle points to.
        target: HandleTarget,
        /// Wire carrier for the handle.
        carrier: S::HandleCarrier,
        /// Whether the slot may be null.
        presence: HandlePresence,
        /// Rust-side receive mode.
        receive: D::Receive,
    },
    /// `Option<P>` for primitive `P` through the surface's scalar-option
    /// path.
    ///
    /// Native packs through a wire buffer. Wasm32 uses one `f64` slot
    /// with `f64::NAN` as the `None` sentinel.
    ScalarOption {
        /// Inner primitive.
        primitive: Primitive,
    },
    /// `Vec<T>` for primitive or direct-record `T` through the
    /// surface's direct-vector path.
    ///
    /// Native uses `VecTransport::pack_vec` / `unpack_vec`. Wasm32 uses
    /// a `(ptr, len, cap, align)` quadruple.
    DirectVec {
        /// Element type.
        element: TypeRef,
    },
}

impl<S: Surface, D: Direction> ParamPlan<S, D> {
    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::Encoded { shape, .. } => Some(*shape),
            _ => None,
        }
    }
}

/// A callable's return slot.
///
/// `meta` carries doc and default metadata that the source method
/// declared. `plan` is the wire shape of the value. A callable that
/// returns nothing carries `ReturnPlan::Void`; there is no separate
/// absence-of-return state.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, D: Direction, D::Codec: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned"
))]
pub struct ReturnDecl<S: Surface, D: Direction> {
    meta: ElementMeta,
    plan: ReturnPlan<S, D>,
}

impl<S: Surface, D: Direction> ReturnDecl<S, D> {
    pub(crate) fn new(meta: ElementMeta, plan: ReturnPlan<S, D>) -> Self {
        Self { meta, plan }
    }

    /// Returns the element metadata.
    pub fn meta(&self) -> &ElementMeta {
        &self.meta
    }

    /// Returns the return plan.
    pub fn plan(&self) -> &ReturnPlan<S, D> {
        &self.plan
    }
}

/// How a return value crosses the boundary in direction `D`.
///
/// The `*ViaReturnSlot` variants occupy the native return slot. The
/// `*ViaOutPointer` variants write the value through a caller-supplied
/// out-pointer parameter while the return slot carries an error status
/// instead. `Void` names the no-value case explicitly.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, S::HandleCarrier: Serialize, D: Direction, D::Codec: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, S::HandleCarrier: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ReturnPlan<S: Surface, D: Direction> {
    /// No return value.
    Void,
    /// Direct value in the return slot.
    DirectViaReturnSlot {
        /// Foreign-side spelling.
        ty: TypeRef,
    },
    /// Encoded value in the return slot.
    EncodedViaReturnSlot {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
    /// Handle in the return slot.
    HandleViaReturnSlot {
        /// Declaration the handle points to.
        target: HandleTarget,
        /// Wire carrier for the handle.
        carrier: S::HandleCarrier,
        /// Whether the slot may be null.
        presence: HandlePresence,
    },
    /// Scalar-option primitive in the return slot.
    ScalarOptionViaReturnSlot {
        /// Inner primitive.
        primitive: Primitive,
    },
    /// Direct-vector in the return slot.
    DirectVecViaReturnSlot {
        /// Element type.
        element: TypeRef,
    },
    /// Direct value through an out-pointer (return slot carries the
    /// error status).
    DirectViaOutPointer {
        /// Foreign-side spelling.
        ty: TypeRef,
    },
    /// Encoded value through an out-pointer.
    EncodedViaOutPointer {
        /// Foreign-side spelling.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
    /// Handle through an out-pointer.
    HandleViaOutPointer {
        /// Declaration the handle points to.
        target: HandleTarget,
        /// Wire carrier for the handle.
        carrier: S::HandleCarrier,
        /// Whether the slot may be null.
        presence: HandlePresence,
    },
}

impl<S: Surface, D: Direction> ReturnPlan<S, D> {
    pub(crate) const fn uses_return_slot(&self) -> bool {
        matches!(
            self,
            Self::DirectViaReturnSlot { .. }
                | Self::EncodedViaReturnSlot { .. }
                | Self::HandleViaReturnSlot { .. }
                | Self::ScalarOptionViaReturnSlot { .. }
                | Self::DirectVecViaReturnSlot { .. }
        )
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::EncodedViaReturnSlot { shape, .. } | Self::EncodedViaOutPointer { shape, .. } => {
                Some(*shape)
            }
            _ => None,
        }
    }

    /// Switches a `*ViaReturnSlot` variant to its `*ViaOutPointer`
    /// counterpart. Called when the matching error channel takes the
    /// return slot.
    pub(crate) fn into_out(self) -> Self {
        match self {
            Self::DirectViaReturnSlot { ty } => Self::DirectViaOutPointer { ty },
            Self::EncodedViaReturnSlot { ty, codec, shape } => {
                Self::EncodedViaOutPointer { ty, codec, shape }
            }
            Self::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => Self::HandleViaOutPointer {
                target,
                carrier,
                presence,
            },
            other => other,
        }
    }
}

/// How a fallible callable reports its error in direction `D`.
///
/// `None` means the callable cannot fail across the boundary.
/// `Status*` carries an integer where one value is success and the
/// others map to specific failures. `Encoded*` carries the failure as
/// a typed payload. The variant suffix names the delivery slot:
/// `ViaReturnSlot` claims the native return slot, `ViaOutPointer`
/// writes through a trailing out-pointer parameter.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "S::BufferShape: Serialize, D: Direction, D::Codec: Serialize",
    deserialize = "S::BufferShape: serde::de::DeserializeOwned, D: Direction, D::Codec: serde::de::DeserializeOwned"
))]
#[non_exhaustive]
pub enum ErrorDecl<S: Surface, D: Direction> {
    /// No error channel.
    None(#[serde(skip)] PhantomData<(S, D)>),
    /// Status integer in the return slot.
    StatusViaReturnSlot {
        /// Status integer representation.
        repr: IntegerRepr,
    },
    /// Status integer in an out-pointer.
    StatusViaOutPointer {
        /// Status integer representation.
        repr: IntegerRepr,
    },
    /// Encoded error in the return slot.
    EncodedViaReturnSlot {
        /// Error type.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
    /// Encoded error in an out-pointer.
    EncodedViaOutPointer {
        /// Error type.
        ty: TypeRef,
        /// Foreign-side codec.
        codec: D::Codec,
        /// Slot layout of the encoded bytes.
        shape: S::BufferShape,
    },
}

impl<S: Surface, D: Direction> ErrorDecl<S, D> {
    /// Builds the `None` variant.
    pub fn none() -> Self {
        Self::None(PhantomData)
    }

    pub(crate) const fn uses_return_slot(&self) -> bool {
        matches!(
            self,
            Self::StatusViaReturnSlot { .. } | Self::EncodedViaReturnSlot { .. }
        )
    }

    pub(crate) fn buffer_shape(&self) -> Option<S::BufferShape> {
        match self {
            Self::EncodedViaReturnSlot { shape, .. } | Self::EncodedViaOutPointer { shape, .. } => {
                Some(*shape)
            }
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

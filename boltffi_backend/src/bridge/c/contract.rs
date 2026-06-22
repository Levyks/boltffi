use std::collections::BTreeMap;

use boltffi_binding::{
    Bindings, CallableDecl, CallbackDecl, CallbackId, ClassDecl, ClosureReturn, ConstantDecl,
    ConstantValueDecl, DeclarationRef, DirectValueType, DirectVectorElementType, Direction,
    EnumDecl, EnumId, ErrorDecl, ExecutionDecl, ExportedCallable, ExportedMethodDecl,
    HandlePresence, HandleTarget, ImportedCallable, ImportedMethodDecl, IncomingParam,
    InitializerDecl, IntoRust, Native, NativeSymbol, OutOfRust, OutgoingParam, ParamDecl,
    ParamDirection, ParamPlan, ParamPlanRender, Primitive, Receive, RecordDecl, RecordId,
    ReturnPlan, ReturnPlanRender, ReturnValueSlot, RustBody, StreamDecl, StreamItemPlan, TypeRef,
    VTableSlot, native,
};

use crate::core::{
    BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result, contract::sealed,
};

use super::names::Names;
use super::{
    C_BRIDGE_LAYER, Enum, Field, Identifier, Parameter, ParameterGroup, ParameterIndex, Record,
    SupportFunctions, Type, name,
};

/// C ABI contract produced for native bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CBridgeContract {
    capabilities: BridgeCapabilities,
    header_path: FilePath,
    support: SupportFunctions,
    direct_records: Vec<Record>,
    source_direct_records: BTreeMap<RecordId, Record>,
    source_c_style_enums: BTreeMap<EnumId, Enum>,
    enums: Vec<Enum>,
    callbacks: Vec<Callback>,
    functions: Vec<Function>,
}

/// A native callback vtable declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Callback {
    id: CallbackId,
    vtable: Record,
    register: Function,
    create_handle: Function,
}

/// A C function declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Function {
    name: Identifier,
    params: Vec<Parameter>,
    parameter_groups: Vec<ParameterGroup>,
    returns: Type,
}

#[derive(Clone, Debug)]
struct Signature {
    names: Names,
    receiver: Vec<Parameter>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReceiverAbi {
    input: Vec<Parameter>,
    writeback: Option<Parameter>,
}

#[derive(Clone, Debug)]
struct PollHandleSymbols {
    start: NativeSymbol,
    poll: NativeSymbol,
    complete: NativeSymbol,
    cancel: NativeSymbol,
    free: NativeSymbol,
    panic_message: NativeSymbol,
}

impl PollHandleSymbols {
    fn new(
        start: &NativeSymbol,
        poll: &NativeSymbol,
        complete: &NativeSymbol,
        cancel: &NativeSymbol,
        free: &NativeSymbol,
        panic_message: &NativeSymbol,
    ) -> Self {
        Self {
            start: start.clone(),
            poll: poll.clone(),
            complete: complete.clone(),
            cancel: cancel.clone(),
            free: free.clone(),
            panic_message: panic_message.clone(),
        }
    }
}

impl CBridgeContract {
    /// Builds the C ABI contract for native bindings.
    pub fn from_bindings(bindings: &Bindings<Native>, header_path: FilePath) -> Result<Self> {
        let names = Names::new(bindings)?;
        let source_direct_records =
            bindings
                .decls()
                .iter()
                .try_fold(BTreeMap::new(), |mut records, decl| {
                    match DeclarationRef::from(decl) {
                        DeclarationRef::Record(RecordDecl::Direct(record)) => {
                            records.insert(record.id(), Record::direct(record, &names)?);
                        }
                        DeclarationRef::Record(RecordDecl::Encoded(_)) => {}
                        DeclarationRef::Record(_) => {
                            return Err(Error::UnexpectedBindingShape {
                                layer: C_BRIDGE_LAYER,
                                shape: "unknown record declaration",
                            });
                        }
                        DeclarationRef::Enum(_)
                        | DeclarationRef::Function(_)
                        | DeclarationRef::Class(_)
                        | DeclarationRef::Callback(_)
                        | DeclarationRef::Stream(_)
                        | DeclarationRef::Constant(_)
                        | DeclarationRef::CustomType(_) => {}
                    }
                    Ok(records)
                })?;
        let direct_records = source_direct_records.values().cloned().collect();
        let enums = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(enumeration) => Some(enumeration),
                DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|enumeration| Enum::from_decl(enumeration, &names))
            .collect::<Result<Vec<_>>>()?;
        let source_c_style_enums = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => Some(enumeration),
                DeclarationRef::Enum(EnumDecl::Data(_))
                | DeclarationRef::Enum(_)
                | DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|enumeration| Ok((enumeration.id(), Enum::c_style(enumeration, &names)?)))
            .collect::<Result<BTreeMap<_, _>>>()?;
        let callbacks = bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Callback(callback) => Some(callback),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|callback| Callback::from_decl(callback, &names))
            .collect::<Result<Vec<_>>>()?;
        let functions = bindings
            .decls()
            .iter()
            .map(|decl| Function::from_decl(DeclarationRef::from(decl), &names))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();

        Ok(Self {
            capabilities: BridgeCapabilities::new().stable(BridgeCapability::CAbi),
            header_path,
            support: SupportFunctions::new()?,
            direct_records,
            source_direct_records,
            source_c_style_enums,
            enums,
            callbacks,
            functions,
        })
    }

    /// Returns the generated C header path.
    pub fn header_path(&self) -> &FilePath {
        &self.header_path
    }

    /// Returns C typedefs for direct source records.
    pub fn direct_records(&self) -> &[Record] {
        &self.direct_records
    }

    /// Returns the C typedef selected for a direct source record.
    pub fn source_direct_record(&self, record: RecordId) -> Option<&Record> {
        self.source_direct_records.get(&record)
    }

    /// Returns C typedefs keyed by direct source record id.
    pub fn source_direct_records(&self) -> &BTreeMap<RecordId, Record> {
        &self.source_direct_records
    }

    /// Returns the C typedef selected for a source C-style enum.
    pub fn source_c_style_enum(&self, enumeration: EnumId) -> Option<&Enum> {
        self.source_c_style_enums.get(&enumeration)
    }

    /// Returns C typedefs keyed by source C-style enum id.
    pub fn source_c_style_enums(&self) -> &BTreeMap<EnumId, Enum> {
        &self.source_c_style_enums
    }

    /// Returns C ABI support functions.
    pub fn support(&self) -> &SupportFunctions {
        &self.support
    }

    /// Returns C enum declarations.
    pub fn enums(&self) -> &[Enum] {
        &self.enums
    }

    /// Returns C callback vtable declarations.
    pub fn callbacks(&self) -> &[Callback] {
        &self.callbacks
    }

    /// Returns C function declarations.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
}

impl BridgeContract for CBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for CBridgeContract {}

impl Field {
    fn callback_method(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        names: &Names,
    ) -> Result<Self> {
        let signature = Signature::new(names, Vec::new());
        if matches!(
            method.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Self::async_callback_method(method, &signature);
        }
        let return_params = signature.callback_return_params(method.callable().returns().plan())?;
        let method_params = signature.imported_params(method.callable().params())?;
        let params = std::iter::once(Type::Uint64)
            .chain(
                return_params
                    .into_iter()
                    .map(|parameter| parameter.ty().clone()),
            )
            .chain(
                method_params
                    .into_iter()
                    .map(|parameter| parameter.ty().clone()),
            )
            .collect();
        Self::new(
            method.target().as_str(),
            Type::FunctionPointer {
                returns: Box::new(signature.callback_return_type(
                    method.callable().returns().plan(),
                    method.callable().error(),
                )?),
                params,
            },
        )
    }

    fn async_callback_method(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        signature: &Signature,
    ) -> Result<Self> {
        let method_params = signature.imported_params(method.callable().params())?;
        let completion = signature.async_completion(
            method.callable().returns().plan(),
            method.callable().error(),
        )?;
        let params = std::iter::once(Type::Uint64)
            .chain(
                method_params
                    .into_iter()
                    .map(|parameter| parameter.ty().clone()),
            )
            .chain([completion, Type::MutPointer(Box::new(Type::Void))])
            .collect();
        Self::new(
            method.target().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params,
            },
        )
    }
}

impl Callback {
    /// Returns the source callback trait id.
    pub const fn id(&self) -> CallbackId {
        self.id
    }

    /// Returns the callback vtable record.
    pub fn vtable(&self) -> &Record {
        &self.vtable
    }

    /// Returns the callback registration function.
    pub fn register(&self) -> &Function {
        &self.register
    }

    /// Returns the callback handle constructor.
    pub fn create_handle(&self) -> &Function {
        &self.create_handle
    }
}

impl Callback {
    fn from_decl(callback: &CallbackDecl<Native>, names: &Names) -> Result<Self> {
        let vtable_name = Identifier::parse(format!("{}VTable", names.callback(callback.id())?))?;
        let vtable = callback.protocol().vtable();
        let free = Field::new(
            vtable.free_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::Uint64],
            },
        )?;
        let clone = Field::new(
            vtable.clone_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Uint64),
                params: vec![Type::Uint64],
            },
        )?;
        let methods = vtable
            .methods()
            .iter()
            .map(|method| Field::callback_method(method, names))
            .collect::<Result<Vec<_>>>()?;
        let vtable = Record::new(
            vtable_name.clone(),
            [free, clone].into_iter().chain(methods).collect(),
        );
        let register = Function::new(
            callback.protocol().register().name().as_str(),
            vec![Parameter::new(
                "vtable",
                Type::ConstPointer(Box::new(Type::Named(vtable_name.clone()))),
            )?],
            Type::Void,
        )?;
        let create_handle = Function::new(
            callback.protocol().create_handle().name().as_str(),
            vec![Parameter::new("handle", Type::Uint64)?],
            Type::CallbackHandle,
        )?;
        Ok(Self {
            id: callback.id(),
            vtable,
            register,
            create_handle,
        })
    }
}

impl Function {
    /// Returns the C symbol name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the parameters in C ABI order.
    pub fn params(&self) -> &[Parameter] {
        &self.params
    }

    /// Returns source-level parameter groups in declaration order.
    pub fn parameter_groups(&self) -> &[ParameterGroup] {
        &self.parameter_groups
    }

    /// Returns the C ABI parameter at the given position.
    pub fn parameter(&self, index: ParameterIndex) -> &Parameter {
        &self.params[index.position()]
    }

    /// Returns the C return type.
    pub fn returns(&self) -> &Type {
        &self.returns
    }
}

impl Function {
    fn from_decl<'decl>(decl: DeclarationRef<'decl, Native>, names: &Names) -> Result<Vec<Self>> {
        match decl {
            DeclarationRef::Function(function) => {
                Self::exported(function.symbol(), function.callable(), Vec::new(), names)
            }
            DeclarationRef::Record(record) => Self::record_functions(record, names),
            DeclarationRef::Enum(enumeration) => Self::enum_functions(enumeration, names),
            DeclarationRef::Class(class) => Self::class_functions(class, names),
            DeclarationRef::Constant(constant) => Self::constant_functions(constant, names),
            DeclarationRef::Stream(stream) => Self::stream_functions(stream, names),
            DeclarationRef::Callback(_) | DeclarationRef::CustomType(_) => Ok(Vec::new()),
        }
    }

    fn record_functions(record: &RecordDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        let (initializers, methods, receiver) = match record {
            RecordDecl::Direct(record) => (
                record.initializers(),
                record.methods(),
                ReceiverAbi::direct("receiver", Type::DirectRecord(names.record(record.id())?))?,
            ),
            RecordDecl::Encoded(record) => (
                record.initializers(),
                record.methods(),
                ReceiverAbi::encoded("receiver")?,
            ),
            _ => {
                return Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "unknown record declaration",
                });
            }
        };
        Self::associated_functions(initializers, methods, receiver, names)
    }

    fn enum_functions(enumeration: &EnumDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        let (initializers, methods, receiver) = match enumeration {
            EnumDecl::CStyle(enumeration) => (
                enumeration.initializers(),
                enumeration.methods(),
                ReceiverAbi::direct(
                    "receiver",
                    Type::CStyleEnum {
                        name: names.enumeration(enumeration.id())?,
                        repr: Box::new(Type::primitive(enumeration.repr().primitive())?),
                    },
                )?,
            ),
            EnumDecl::Data(enumeration) => (
                enumeration.initializers(),
                enumeration.methods(),
                ReceiverAbi::encoded("receiver")?,
            ),
            _ => {
                return Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "unknown enum declaration",
                });
            }
        };
        Self::associated_functions(initializers, methods, receiver, names)
    }

    fn class_functions(class: &ClassDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        let receiver = ReceiverAbi::plain([Parameter::new(
            "receiver",
            Type::handle_carrier(class.handle())?,
        )?]);
        let release = Self::new(
            class.release().name().as_str(),
            vec![Parameter::new(
                "handle",
                Type::handle_carrier(class.handle())?,
            )?],
            Type::Void,
        )?;
        let functions =
            Self::associated_functions(class.initializers(), class.methods(), receiver, names)?;
        Ok(std::iter::once(release).chain(functions).collect())
    }

    fn constant_functions(constant: &ConstantDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        match constant.value() {
            ConstantValueDecl::Inline { .. } => Ok(Vec::new()),
            ConstantValueDecl::Accessor { symbol, callable } => {
                Self::exported(symbol, callable, Vec::new(), names)
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown constant value declaration",
            }),
        }
    }

    fn stream_functions(stream: &StreamDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        let protocol = stream.protocol();
        let subscription = Type::handle_carrier(stream.handle())?;
        let subscribe_params = stream
            .owner()
            .map(|owner| {
                names
                    .class_handle(owner)
                    .and_then(Type::handle_carrier)
                    .and_then(|ty| Parameter::new("receiver", ty))
            })
            .transpose()?
            .into_iter()
            .collect();
        let pop_batch = match stream.item() {
            StreamItemPlan::Direct { ty, .. } => Self::new(
                protocol.pop_batch().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone())?,
                    Parameter::new(
                        "output_ptr",
                        Type::MutPointer(Box::new(names.direct_value(ty)?)),
                    )?,
                    Parameter::new("output_capacity", Type::PointerWidth)?,
                ],
                Type::PointerWidth,
            )?,
            StreamItemPlan::Encoded { shape, .. } => Self::new(
                protocol.pop_batch().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone())?,
                    Parameter::new("max_count", Type::PointerWidth)?,
                ],
                Signature::new(names, Vec::new()).encoded_return(*shape)?,
            )?,
            _ => {
                return Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "unknown stream item plan",
                });
            }
        };
        Ok(vec![
            Self::new(
                protocol.subscribe().name().as_str(),
                subscribe_params,
                subscription.clone(),
            )?,
            pop_batch,
            Self::new(
                protocol.wait().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone())?,
                    Parameter::new("timeout_milliseconds", Type::Uint32)?,
                ],
                Type::WaitResult,
            )?,
            Self::new(
                protocol.poll().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone())?,
                    Parameter::new("callback_data", Type::Uint64)?,
                    Parameter::new(
                        "callback",
                        Type::FunctionPointer {
                            returns: Box::new(Type::Void),
                            params: vec![Type::Uint64, Type::StreamPollResult],
                        },
                    )?,
                ],
                Type::Void,
            )?,
            Self::new(
                protocol.unsubscribe().name().as_str(),
                vec![Parameter::new("subscription", subscription.clone())?],
                Type::Void,
            )?,
            Self::new(
                protocol.free().name().as_str(),
                vec![Parameter::new("subscription", subscription)?],
                Type::Void,
            )?,
        ])
    }

    fn associated_functions(
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: ReceiverAbi,
        names: &Names,
    ) -> Result<Vec<Self>> {
        let initializers = initializers
            .iter()
            .map(|initializer| {
                Self::exported(
                    initializer.symbol(),
                    initializer.callable(),
                    Vec::new(),
                    names,
                )
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten();
        let methods = methods
            .iter()
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|receive| receiver.parameters(receive))
                    .unwrap_or_default();
                Self::exported(method.target(), method.callable(), receiver, names)
            })
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten();
        Ok(initializers.chain(methods).collect())
    }

    fn exported(
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver: impl IntoIterator<Item = Parameter>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        Signature::new(names, receiver).exported(symbol, callable)
    }

    /// Creates a C function declaration.
    pub fn new(name: impl Into<String>, params: Vec<Parameter>, returns: Type) -> Result<Self> {
        let parameter_groups = ParameterGroup::from_params(&params)?;
        Ok(Self {
            name: Identifier::parse(name)?,
            params,
            parameter_groups,
            returns,
        })
    }
}

enum DirectVectorElementAbi {
    TypedElement(Type),
    PackedBytes,
}

impl DirectVectorElementAbi {
    fn new(element: &DirectVectorElementType) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => {
                Type::primitive(primitive.primitive()).map(Self::TypedElement)
            }
            DirectVectorElementType::Record(_) => Ok(Self::PackedBytes),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "direct vector element",
            }),
        }
    }
}

struct ValueParameter {
    signature: Signature,
    name: String,
}

struct ReturnParameters {
    signature: Signature,
}

struct CallableReturnType {
    signature: Signature,
}

struct InfallibleCallbackReturnType {
    signature: Signature,
}

struct AsyncCallbackPayloadType {
    signature: Signature,
}

struct FallibleAsyncCallbackSuccess;

trait EncodedWritebackReceive {
    fn needs_encoded_writeback(self) -> bool;
}

impl EncodedWritebackReceive for Receive {
    fn needs_encoded_writeback(self) -> bool {
        self == Receive::ByMutRef
    }
}

impl EncodedWritebackReceive for () {
    fn needs_encoded_writeback(self) -> bool {
        false
    }
}

impl CallableReturnType {
    fn direct_slot(&self, slot: ReturnValueSlot, ty: &DirectValueType) -> Result<Type> {
        match slot {
            ReturnValueSlot::ReturnSlot => self.signature.names.direct_value(ty),
            ReturnValueSlot::OutPointer => Ok(Type::Status),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct return slot",
            }),
        }
    }

    fn encoded_slot(&self, slot: ReturnValueSlot, shape: native::BufferShape) -> Result<Type> {
        match slot {
            ReturnValueSlot::ReturnSlot => self.signature.encoded_return(shape),
            ReturnValueSlot::OutPointer => Ok(Type::Status),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded return slot",
            }),
        }
    }

    fn handle_slot(&self, slot: ReturnValueSlot, carrier: native::HandleCarrier) -> Result<Type> {
        match slot {
            ReturnValueSlot::ReturnSlot => Type::handle_carrier(carrier),
            ReturnValueSlot::OutPointer => Ok(Type::Status),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle return slot",
            }),
        }
    }

    fn buffer(&self) -> Type {
        Type::Buffer
    }

    fn status(&self) -> Type {
        Type::Status
    }
}

impl<'plan, D> ParamPlanRender<'plan, Native, D> for ValueParameter
where
    D: Direction,
    D::Receive: EncodedWritebackReceive,
{
    type Output = Result<Vec<Parameter>>;

    fn direct(&mut self, ty: &'plan DirectValueType, _: D::Receive) -> Self::Output {
        Ok(vec![Parameter::new(
            self.name.as_str(),
            self.signature.names.direct_value(ty)?,
        )?])
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
        receive: D::Receive,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => {
                let mut parameters = vec![
                    Parameter::byte_pointer(&self.name)?,
                    Parameter::byte_length(&self.name)?,
                ];
                if receive.needs_encoded_writeback() {
                    parameters.push(Parameter::new(
                        format!("{}_out", self.name),
                        Type::MutPointer(Box::new(Type::Buffer)),
                    )?);
                }
                Ok(parameters)
            }
            native::BufferShape::Buffer | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded parameter shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "native encoded parameter shape",
            }),
        }
    }

    fn handle(
        &mut self,
        _: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
        _: D::Receive,
    ) -> Self::Output {
        Ok(vec![Parameter::new(
            self.name.as_str(),
            Type::handle_carrier(carrier)?,
        )?])
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(vec![
            Parameter::byte_pointer(&self.name)?,
            Parameter::byte_length(&self.name)?,
        ])
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        self.signature.direct_vec_param(&self.name, element)
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for ReturnParameters
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Vec<Parameter>>;

    fn void(&mut self) -> Self::Output {
        Ok(Vec::new())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(vec![Parameter::new(
                "return_out",
                Type::MutPointer(Box::new(self.signature.names.direct_value(ty)?)),
            )?]),
            ReturnValueSlot::ReturnSlot => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct return slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => self.signature.encoded_out("return_out", shape),
            ReturnValueSlot::ReturnSlot => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded return slot",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(vec![Parameter::new(
                "return_out",
                Type::MutPointer(Box::new(Type::handle_carrier(carrier)?)),
            )?]),
            ReturnValueSlot::ReturnSlot => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle return slot",
            }),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(Vec::new())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(Vec::new())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Ok(vec![Parameter::new(
            "return_out",
            Type::MutPointer(Box::new(Type::Void)),
        )?])
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for CallableReturnType
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Type>;

    fn void(&mut self) -> Self::Output {
        Ok(Type::Status)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        self.direct_slot(slot, ty)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        self.encoded_slot(slot, shape)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        self.handle_slot(slot, carrier)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(self.buffer())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(self.buffer())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Ok(self.status())
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for InfallibleCallbackReturnType
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Type>;

    fn void(&mut self) -> Self::Output {
        Ok(Type::Void)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        CallableReturnType {
            signature: self.signature.clone(),
        }
        .direct_slot(slot, ty)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        CallableReturnType {
            signature: self.signature.clone(),
        }
        .encoded_slot(slot, shape)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        CallableReturnType {
            signature: self.signature.clone(),
        }
        .handle_slot(slot, carrier)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(CallableReturnType {
            signature: self.signature.clone(),
        }
        .buffer())
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(CallableReturnType {
            signature: self.signature.clone(),
        }
        .buffer())
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Ok(CallableReturnType {
            signature: self.signature.clone(),
        }
        .status())
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for AsyncCallbackPayloadType
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<Option<Type>>;

    fn void(&mut self) -> Self::Output {
        Ok(None)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Some(self.signature.names.direct_value(ty)?)),
            ReturnValueSlot::OutPointer => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct async callback return slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        shape: native::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Some(self.signature.encoded_return(shape)?)),
            ReturnValueSlot::OutPointer => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded async callback return slot",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        carrier: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(Some(Type::handle_carrier(carrier)?)),
            ReturnValueSlot::OutPointer => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle async callback return slot",
            }),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(Some(Type::Buffer))
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(Some(Type::Buffer))
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "async callback closure return",
        })
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, D> for FallibleAsyncCallbackSuccess
where
    D: Direction,
    D::Opposite: ParamDirection<Native>,
{
    type Output = Result<()>;

    fn void(&mut self) -> Self::Output {
        Ok(())
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown direct fallible async callback success slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan D::Codec,
        _: native::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown encoded fallible async callback success slot",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown handle fallible async callback success slot",
            }),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "fallible async callback success slot",
        })
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "fallible async callback success slot",
        })
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, D>) -> Self::Output {
        Err(Error::UnsupportedCAbi {
            shape: "async callback closure success",
        })
    }
}

impl Signature {
    fn new(names: &Names, receiver: impl IntoIterator<Item = Parameter>) -> Self {
        Self {
            names: names.clone(),
            receiver: receiver.into_iter().collect(),
        }
    }

    fn exported(
        self,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
    ) -> Result<Vec<Function>> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => self
                .synchronous(symbol.name().as_str(), callable)
                .map(|function| vec![function]),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => self.async_poll_handle(
                callable,
                PollHandleSymbols::new(symbol, poll, complete, cancel, free, panic_message),
            ),
            ExecutionDecl::Asynchronous(
                native::AsyncProtocol::NativeFuture | native::AsyncProtocol::Continuation { .. },
            ) => Err(Error::UnsupportedCAbi {
                shape: "native async protocol",
            }),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::CallbackCompletion) => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "callback completion protocol on exported callable",
                })
            }
            ExecutionDecl::Asynchronous(_) => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native async protocol",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown execution declaration",
            }),
        }
    }

    fn synchronous(&self, name: &str, callable: &ExportedCallable<Native>) -> Result<Function> {
        let params = self
            .receiver
            .clone()
            .into_iter()
            .chain(self.exported_params(callable.params())?)
            .chain(self.return_params(callable.returns().plan())?)
            .chain(self.error_params(callable.error())?)
            .collect();
        let returns = self.return_type(callable.returns().plan(), callable.error())?;
        Function::new(name, params, returns)
    }

    fn async_poll_handle(
        &self,
        callable: &ExportedCallable<Native>,
        symbols: PollHandleSymbols,
    ) -> Result<Vec<Function>> {
        let start = Function::new(
            symbols.start.name().as_str(),
            self.receiver
                .clone()
                .into_iter()
                .chain(self.exported_params(callable.params())?)
                .collect(),
            Type::FutureHandle,
        )?;
        let complete_params = std::iter::once(Parameter::new("handle", Type::FutureHandle)?)
            .chain([Parameter::new(
                "out_status",
                Type::MutPointer(Box::new(Type::Status)),
            )?])
            .chain(self.return_params(callable.returns().plan())?)
            .collect();
        Ok(vec![
            start,
            Function::new(
                symbols.poll.name().as_str(),
                vec![
                    Parameter::new("handle", Type::FutureHandle)?,
                    Parameter::new("callback_data", Type::Uint64)?,
                    Parameter::new(
                        "callback",
                        Type::FunctionPointer {
                            returns: Box::new(Type::Void),
                            params: vec![Type::Uint64, Type::Int8],
                        },
                    )?,
                ],
                Type::Void,
            )?,
            Function::new(
                symbols.complete.name().as_str(),
                complete_params,
                self.async_complete_return(callable.returns().plan(), callable.error())?,
            )?,
            Function::new(
                symbols.panic_message.name().as_str(),
                vec![Parameter::new("handle", Type::FutureHandle)?],
                Type::Buffer,
            )?,
            Function::new(
                symbols.cancel.name().as_str(),
                vec![Parameter::new("handle", Type::FutureHandle)?],
                Type::Void,
            )?,
            Function::new(
                symbols.free.name().as_str(),
                vec![Parameter::new("handle", Type::FutureHandle)?],
                Type::Void,
            )?,
        ])
    }

    fn exported_params(&self, params: &[ParamDecl<Native, IntoRust>]) -> Result<Vec<Parameter>> {
        params
            .iter()
            .map(|param| {
                let name = name::Spelling::new(param.name()).parameter();
                match param.payload() {
                    IncomingParam::Value(plan) => self.value_param(&name, plan),
                    IncomingParam::Closure(closure) => {
                        self.incoming_closure_param(&name, closure.invoke())
                    }
                }
            })
            .collect::<Result<Vec<_>>>()
            .map(Vec::into_iter)
            .map(|parameters| parameters.flatten().collect())
    }

    fn imported_params(&self, params: &[ParamDecl<Native, OutOfRust>]) -> Result<Vec<Parameter>> {
        params
            .iter()
            .map(|param| {
                let name = name::Spelling::new(param.name()).parameter();
                match param.payload() {
                    OutgoingParam::Value(plan) => self.value_param(&name, plan),
                    OutgoingParam::Closure(closure) => {
                        self.outgoing_closure_param(&name, closure.invoke())
                    }
                }
            })
            .collect::<Result<Vec<_>>>()
            .map(Vec::into_iter)
            .map(|parameters| parameters.flatten().collect())
    }

    fn value_param<D>(&self, name: &str, plan: &ParamPlan<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Receive: EncodedWritebackReceive,
    {
        plan.render_with(&mut ValueParameter {
            signature: self.clone(),
            name: name.to_owned(),
        })
    }

    fn direct_vec_param(
        &self,
        name: &str,
        element: &DirectVectorElementType,
    ) -> Result<Vec<Parameter>> {
        match DirectVectorElementAbi::new(element)? {
            DirectVectorElementAbi::TypedElement(element) => Ok(vec![
                Parameter::new(format!("{name}_ptr"), Type::ConstPointer(Box::new(element)))?,
                Parameter::new(format!("{name}_len"), Type::PointerWidth)?,
            ]),
            DirectVectorElementAbi::PackedBytes => Ok(vec![
                Parameter::new(
                    format!("{name}_ptr"),
                    Type::ConstPointer(Box::new(Type::Uint8)),
                )?,
                Parameter::new(format!("{name}_byte_len"), Type::PointerWidth)?,
            ]),
        }
    }

    fn incoming_closure_param(
        &self,
        name: &str,
        invoke: &ImportedCallable<Native>,
    ) -> Result<Vec<Parameter>> {
        self.closure_param(
            name,
            self.imported_params(invoke.params())?,
            invoke.returns().plan(),
            invoke.error(),
        )
    }

    fn outgoing_closure_param(
        &self,
        name: &str,
        invoke: &CallableDecl<Native, RustBody>,
    ) -> Result<Vec<Parameter>> {
        self.closure_param(
            name,
            self.exported_params(invoke.params())?,
            invoke.returns().plan(),
            invoke.error(),
        )
    }

    fn closure_param<D>(
        &self,
        name: &str,
        params: Vec<Parameter>,
        returns: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        Ok(vec![
            Parameter::closure_call(
                name,
                Type::FunctionPointer {
                    returns: Box::new(self.callback_return_type(returns, error)?),
                    params: std::iter::once(Type::MutPointer(Box::new(Type::Void)))
                        .chain(params.into_iter().map(|parameter| parameter.ty().clone()))
                        .chain(
                            self.callback_return_params(returns)?
                                .into_iter()
                                .map(|parameter| parameter.ty().clone()),
                        )
                        .collect(),
                },
            )?,
            Parameter::closure_context(name)?,
            Parameter::closure_release(name)?,
        ])
    }

    fn return_params<D>(&self, plan: &ReturnPlan<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut ReturnParameters {
            signature: self.clone(),
        })
    }

    fn error_params<D>(&self, error: &ErrorDecl<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
    {
        match error {
            ErrorDecl::StatusViaOutPointer { .. } => Ok(vec![Parameter::new(
                "error_out",
                Type::MutPointer(Box::new(Type::Status)),
            )?]),
            ErrorDecl::EncodedViaOutPointer { shape, .. } => self.encoded_out("error_out", *shape),
            ErrorDecl::None(_)
            | ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::EncodedViaReturnSlot { .. } => Ok(Vec::new()),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown error declaration",
            }),
        }
    }

    fn return_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::StatusViaReturnSlot { repr } => Type::primitive(repr.primitive()),
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => self.encoded_return(*shape),
            ErrorDecl::None(_)
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => self.return_slot_type(plan),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown error declaration",
            }),
        }
    }

    fn return_slot_type<D>(&self, plan: &ReturnPlan<Native, D>) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut CallableReturnType {
            signature: self.clone(),
        })
    }

    fn async_complete_return<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => self.encoded_return(*shape),
            ErrorDecl::None(_) => plan.render_with(&mut InfallibleCallbackReturnType {
                signature: self.clone(),
            }),
            ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => Err(Error::UnsupportedCAbi {
                shape: "async error channel",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown async error channel",
            }),
        }
    }

    fn encoded_return(&self, shape: native::BufferShape) -> Result<Type> {
        match shape {
            native::BufferShape::Buffer => Ok(Type::Buffer),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded return shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native encoded return shape",
            }),
        }
    }

    fn encoded_out(&self, name: &str, shape: native::BufferShape) -> Result<Vec<Parameter>> {
        match shape {
            native::BufferShape::Buffer => Ok(vec![Parameter::new(
                name,
                Type::MutPointer(Box::new(Type::Buffer)),
            )?]),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnexpectedBindingShape {
                    layer: C_BRIDGE_LAYER,
                    shape: "native encoded out-pointer shape",
                })
            }
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native encoded out-pointer shape",
            }),
        }
    }

    fn callback_return_params<D>(&self, plan: &ReturnPlan<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        self.return_params(plan)
    }

    fn callback_return_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::None(_) => plan.render_with(&mut InfallibleCallbackReturnType {
                signature: self.clone(),
            }),
            _ => self.return_type(plan, error),
        }
    }

    fn async_completion<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        let result = self.async_callback_payload_type(plan, error)?;
        Ok(Type::FunctionPointer {
            returns: Box::new(Type::Void),
            params: std::iter::once(Type::MutPointer(Box::new(Type::Void)))
                .chain([Type::Status])
                .chain(result)
                .collect(),
        })
    }

    fn async_callback_payload_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
        error: &ErrorDecl<Native, D>,
    ) -> Result<Option<Type>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match error {
            ErrorDecl::None(_) => self.infallible_async_callback_payload_type(plan),
            ErrorDecl::EncodedViaReturnSlot { shape, .. } => {
                self.encoded_return(*shape)?;
                self.validate_fallible_async_callback_success(plan)?;
                Ok(Some(Type::Buffer))
            }
            ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => Err(Error::UnsupportedCAbi {
                shape: "async callback error channel",
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown async callback error channel",
            }),
        }
    }

    fn infallible_async_callback_payload_type<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
    ) -> Result<Option<Type>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut AsyncCallbackPayloadType {
            signature: self.clone(),
        })
    }

    fn validate_fallible_async_callback_success<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
    ) -> Result<()>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        plan.render_with(&mut FallibleAsyncCallbackSuccess)
    }
}

impl ReceiverAbi {
    fn plain(params: impl IntoIterator<Item = Parameter>) -> Self {
        Self {
            input: params.into_iter().collect(),
            writeback: None,
        }
    }

    fn direct(name: &str, ty: Type) -> Result<Self> {
        Ok(Self {
            input: vec![Parameter::new(name, ty.clone())?],
            writeback: Some(Parameter::new(
                format!("{name}_out"),
                Type::MutPointer(Box::new(ty)),
            )?),
        })
    }

    fn encoded(name: &str) -> Result<Self> {
        Ok(Self {
            input: vec![
                Parameter::new(
                    format!("{name}_ptr"),
                    Type::ConstPointer(Box::new(Type::Uint8)),
                )?,
                Parameter::new(format!("{name}_len"), Type::PointerWidth)?,
            ],
            writeback: Some(Parameter::new(
                format!("{name}_out"),
                Type::MutPointer(Box::new(Type::Buffer)),
            )?),
        })
    }

    fn parameters(&self, receive: Receive) -> Vec<Parameter> {
        self.input
            .iter()
            .cloned()
            .chain(
                matches!(receive, Receive::ByMutRef)
                    .then(|| self.writeback.clone())
                    .flatten(),
            )
            .collect()
    }
}

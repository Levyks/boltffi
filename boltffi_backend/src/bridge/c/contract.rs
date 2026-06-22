use std::collections::BTreeMap;

use boltffi_binding::{
    Bindings, CStyleEnumDecl, CallableDecl, CallbackDecl, CallbackId, ClassDecl, ClassId,
    ClosureReturn, ConstantDecl, ConstantValueDecl, Decl, DeclarationRef, DirectRecordDecl,
    DirectValueType, DirectVectorElementType, Direction, EnumDecl, EnumId, ErrorDecl,
    ExecutionDecl, ExportedCallable, ExportedMethodDecl, HandlePresence, HandleTarget,
    ImportedCallable, ImportedMethodDecl, IncomingParam, InitializerDecl, IntoRust, Native,
    NativeSymbol, OutOfRust, OutgoingParam, ParamDecl, ParamDirection, ParamPlan, ParamPlanRender,
    Primitive, Receive, RecordDecl, RecordId, ReturnPlan, ReturnPlanRender, ReturnValueSlot,
    RustBody, StreamDecl, StreamId, StreamItemPlan, TypeRef, VTableSlot, native,
};

use crate::core::{
    BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result, contract::sealed,
};

use super::{Identifier, name};

const C_BRIDGE_LAYER: &str = "c bridge";
const C_BRIDGE_CONTRACT: &str = "c";

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

/// A C record typedef.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Record {
    name: Identifier,
    fields: Vec<Field>,
}

/// A C field declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Field {
    name: Identifier,
    ty: Type,
}

/// C ABI support functions supplied by the BoltFFI runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct SupportFunctions {
    functions: Vec<Function>,
}

/// A C enum typedef with integer-valued variants.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Enum {
    name: Identifier,
    repr: Type,
    variants: Vec<EnumVariant>,
}

/// A C enum variant constant.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct EnumVariant {
    name: Identifier,
    value: i128,
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

/// A C function parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Parameter {
    name: Identifier,
    ty: Type,
    role: ParameterRole,
}

/// Position of a C ABI parameter in a function declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct ParameterIndex {
    index: usize,
}

/// Source-level parameter group represented by one or more C ABI parameters.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParameterGroup {
    /// One source parameter maps to one C ABI parameter.
    Value(ParameterIndex),
    /// One source parameter maps to a borrowed byte pointer and byte length.
    ByteSlice(ByteSliceParameter),
    /// One closure parameter maps to call, context, and release C ABI parameters.
    Closure(ClosureParameter),
}

/// C ABI parameters that carry one borrowed byte slice argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ByteSliceParameter {
    name: Identifier,
    pointer: ParameterIndex,
    length: ParameterIndex,
}

/// C ABI parameters that carry one closure argument.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureParameter {
    name: Identifier,
    call: ParameterIndex,
    context: ParameterIndex,
    release: ParameterIndex,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParameterRole {
    Value,
    BytePointer(Identifier),
    ByteLength(Identifier),
    ClosureCall(Identifier),
    ClosureContext(Identifier),
    ClosureRelease(Identifier),
}

/// A C ABI type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Type {
    /// `void`.
    Void,
    /// `bool`.
    Bool,
    /// `int8_t`.
    Int8,
    /// `uint8_t`.
    Uint8,
    /// `int16_t`.
    Int16,
    /// `uint16_t`.
    Uint16,
    /// `int32_t`.
    Int32,
    /// `uint32_t`.
    Uint32,
    /// `int64_t`.
    Int64,
    /// `uint64_t`.
    Uint64,
    /// `float`.
    Float32,
    /// `double`.
    Float64,
    /// `intptr_t`.
    SignedPointerWidth,
    /// `uintptr_t`.
    PointerWidth,
    /// `FfiStatus`.
    Status,
    /// `FfiBuf_u8`.
    Buffer,
    /// `FfiString`.
    String,
    /// `FfiSpan`.
    Span,
    /// `RustFutureHandle`.
    FutureHandle,
    /// `StreamPollResult`.
    StreamPollResult,
    /// `WaitResult`.
    WaitResult,
    /// `BoltFFICallbackHandle`.
    CallbackHandle,
    /// A generated named C type.
    Named(Identifier),
    /// A generated direct record typedef passed by value.
    DirectRecord(Identifier),
    /// A generated C-style enum typedef passed by value.
    CStyleEnum {
        /// Generated enum typedef name.
        name: Identifier,
        /// Integer representation used by the typedef.
        repr: Box<Type>,
    },
    /// Pointer to const data.
    ConstPointer(Box<Type>),
    /// Pointer to mutable data.
    MutPointer(Box<Type>),
    /// C function pointer.
    FunctionPointer {
        /// Function pointer return type.
        returns: Box<Type>,
        /// Function pointer parameters.
        params: Vec<Type>,
    },
}

#[derive(Clone, Debug, Default)]
struct Names {
    direct_records: BTreeMap<RecordId, Identifier>,
    enums: BTreeMap<EnumId, Identifier>,
    c_style_enum_reprs: BTreeMap<EnumId, Type>,
    classes: BTreeMap<ClassId, Identifier>,
    class_handles: BTreeMap<ClassId, native::HandleCarrier>,
    callbacks: BTreeMap<CallbackId, Identifier>,
    streams: BTreeMap<StreamId, Identifier>,
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

impl Record {
    /// Returns the C typedef name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the C fields in declaration order.
    pub fn fields(&self) -> &[Field] {
        &self.fields
    }
}

impl Record {
    fn direct(record: &DirectRecordDecl<Native>, names: &Names) -> Result<Self> {
        let name = names.record(record.id())?;
        let fields = record
            .fields()
            .iter()
            .map(|field| {
                Field::new(
                    name::Field::new(field.key()).spelling()?,
                    Type::primitive(field.ty().primitive())?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { name, fields })
    }
}

impl Field {
    /// Returns the field name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the field type.
    pub fn ty(&self) -> &Type {
        &self.ty
    }
}

impl Field {
    fn new(name: impl Into<String>, ty: Type) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(name)?,
            ty,
        })
    }

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
            .chain(return_params.into_iter().map(|parameter| parameter.ty))
            .chain(method_params.into_iter().map(|parameter| parameter.ty))
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
            .chain(method_params.into_iter().map(|parameter| parameter.ty))
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

impl SupportFunctions {
    /// Creates the C ABI support function set.
    pub fn new() -> Result<Self> {
        Ok(Self {
            functions: vec![
                Function::new(
                    "boltffi_free_string",
                    vec![Parameter::new("string", Type::String)?],
                    Type::Void,
                )?,
                Function::new(
                    "boltffi_free_buf",
                    vec![Parameter::new("buf", Type::Buffer)?],
                    Type::Void,
                )?,
                Function::new(
                    "boltffi_buf_from_bytes",
                    vec![
                        Parameter::new("ptr", Type::ConstPointer(Box::new(Type::Uint8)))?,
                        Parameter::new("len", Type::PointerWidth)?,
                    ],
                    Type::Buffer,
                )?,
                Function::new(
                    "boltffi_last_error_message",
                    vec![Parameter::new(
                        "out",
                        Type::MutPointer(Box::new(Type::String)),
                    )?],
                    Type::Status,
                )?,
                Function::new("boltffi_clear_last_error", Vec::new(), Type::Void)?,
            ],
        })
    }

    /// Returns C ABI support functions.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    /// Returns the C ABI support function that releases a BoltFFI buffer.
    pub fn buffer_free(&self) -> Result<&Function> {
        self.function("boltffi_free_buf", "missing C free buffer support symbol")
    }

    fn function(&self, name: &str, shape: &'static str) -> Result<&Function> {
        self.functions
            .iter()
            .find(|function| function.name() == name)
            .ok_or(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape,
            })
    }
}

impl Enum {
    /// Returns the C typedef name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the C integer representation.
    pub fn repr(&self) -> &Type {
        &self.repr
    }

    /// Returns the enum constants in declaration order.
    pub fn variants(&self) -> &[EnumVariant] {
        &self.variants
    }
}

impl Enum {
    fn from_decl(enumeration: &EnumDecl<Native>, names: &Names) -> Result<Self> {
        match enumeration {
            EnumDecl::CStyle(enumeration) => Self::c_style(enumeration, names),
            EnumDecl::Data(enumeration) => Ok(Self {
                name: names.enumeration(enumeration.id())?,
                repr: Type::Uint32,
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| {
                        EnumVariant::new(
                            name::EnumConstant::new(enumeration.name(), variant.name()).spelling(),
                            i128::from(variant.tag().get()),
                        )
                    })
                    .collect::<Result<Vec<_>>>()?,
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown enum declaration",
            }),
        }
    }

    fn c_style(enumeration: &CStyleEnumDecl<Native>, names: &Names) -> Result<Self> {
        Ok(Self {
            name: names.enumeration(enumeration.id())?,
            repr: Type::primitive(enumeration.repr().primitive())?,
            variants: enumeration
                .variants()
                .iter()
                .map(|variant| {
                    EnumVariant::new(
                        name::EnumConstant::new(enumeration.name(), variant.name()).spelling(),
                        variant.discriminant().get(),
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl EnumVariant {
    /// Returns the C constant name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the integer constant value.
    pub const fn value(&self) -> i128 {
        self.value
    }
}

impl EnumVariant {
    fn new(name: impl Into<String>, value: i128) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(name)?,
            value,
        })
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
        let vtable = Record {
            name: vtable_name.clone(),
            fields: [free, clone].into_iter().chain(methods).collect(),
        };
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

    fn new(name: impl Into<String>, params: Vec<Parameter>, returns: Type) -> Result<Self> {
        let parameter_groups = ParameterGroup::from_params(&params)?;
        Ok(Self {
            name: Identifier::parse(name)?,
            params,
            parameter_groups,
            returns,
        })
    }
}

impl Parameter {
    /// Returns the parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the parameter type.
    pub fn ty(&self) -> &Type {
        &self.ty
    }
}

impl Parameter {
    fn new(name: impl Into<String>, ty: Type) -> Result<Self> {
        Self::with_role(name, ty, ParameterRole::Value)
    }

    fn byte_pointer(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_ptr"),
            Type::ConstPointer(Box::new(Type::Uint8)),
            ParameterRole::BytePointer(Identifier::escape(name)?),
        )
    }

    fn byte_length(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_len"),
            Type::PointerWidth,
            ParameterRole::ByteLength(Identifier::escape(name)?),
        )
    }

    fn closure_call(name: &str, ty: Type) -> Result<Self> {
        Self::with_role(
            format!("{name}_call"),
            ty,
            ParameterRole::ClosureCall(Identifier::escape(name)?),
        )
    }

    fn closure_context(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_context"),
            Type::MutPointer(Box::new(Type::Void)),
            ParameterRole::ClosureContext(Identifier::escape(name)?),
        )
    }

    fn closure_release(name: &str) -> Result<Self> {
        Self::with_role(
            format!("{name}_release"),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::MutPointer(Box::new(Type::Void))],
            },
            ParameterRole::ClosureRelease(Identifier::escape(name)?),
        )
    }

    fn with_role(name: impl Into<String>, ty: Type, role: ParameterRole) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(name)?,
            ty,
            role,
        })
    }
}

impl ParameterIndex {
    /// Returns the zero-based C ABI parameter position.
    pub const fn position(self) -> usize {
        self.index
    }

    const fn new(index: usize) -> Self {
        Self { index }
    }
}

impl ParameterGroup {
    fn from_params(params: &[Parameter]) -> Result<Vec<Self>> {
        let mut index = 0;
        std::iter::from_fn(|| {
            (index < params.len()).then(|| {
                let group = Self::from_param(params, index);
                index += group.as_ref().map_or(1, Self::width);
                group
            })
        })
        .collect()
    }

    fn from_param(params: &[Parameter], index: usize) -> Result<Self> {
        match &params[index].role {
            ParameterRole::Value => Ok(Self::Value(ParameterIndex::new(index))),
            ParameterRole::BytePointer(name) => {
                ByteSliceParameter::from_params(params, index, name).map(Self::ByteSlice)
            }
            ParameterRole::ByteLength(_) => Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "byte slice parameter group does not start with pointer parameter",
            }),
            ParameterRole::ClosureCall(name) => {
                ClosureParameter::from_params(params, index, name).map(Self::Closure)
            }
            ParameterRole::ClosureContext(_) | ParameterRole::ClosureRelease(_) => {
                Err(Error::BrokenBridgeContract {
                    bridge: C_BRIDGE_CONTRACT,
                    invariant: "closure parameter group does not start with call parameter",
                })
            }
        }
    }

    fn width(&self) -> usize {
        match self {
            Self::Value(_) => 1,
            Self::ByteSlice(_) => 2,
            Self::Closure(_) => 3,
        }
    }
}

impl ByteSliceParameter {
    /// Returns the source parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the byte pointer parameter position.
    pub const fn pointer(&self) -> ParameterIndex {
        self.pointer
    }

    /// Returns the byte length parameter position.
    pub const fn length(&self) -> ParameterIndex {
        self.length
    }
}

impl ByteSliceParameter {
    fn from_params(params: &[Parameter], pointer: usize, name: &Identifier) -> Result<Self> {
        let length = pointer + 1;
        let length_role = params.get(length).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "byte slice parameter group is missing length parameter",
            },
        )?;

        if !length_role.is_byte_length(name) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "byte slice parameter group has mismatched length parameter",
            });
        }

        Ok(Self {
            name: name.clone(),
            pointer: ParameterIndex::new(pointer),
            length: ParameterIndex::new(length),
        })
    }
}

impl ClosureParameter {
    /// Returns the source parameter name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the call function pointer parameter position.
    pub const fn call(&self) -> ParameterIndex {
        self.call
    }

    /// Returns the callback context parameter position.
    pub const fn context(&self) -> ParameterIndex {
        self.context
    }

    /// Returns the callback release function parameter position.
    pub const fn release(&self) -> ParameterIndex {
        self.release
    }
}

impl ClosureParameter {
    fn from_params(params: &[Parameter], call: usize, name: &Identifier) -> Result<Self> {
        let context = call + 1;
        let release = call + 2;
        let context_role = params.get(context).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure parameter group is missing context parameter",
            },
        )?;
        let release_role = params.get(release).map(|parameter| &parameter.role).ok_or(
            Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure parameter group is missing release parameter",
            },
        )?;

        if !context_role.is_closure_context(name) || !release_role.is_closure_release(name) {
            return Err(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "closure parameter group has mismatched context or release parameter",
            });
        }

        Ok(Self {
            name: name.clone(),
            call: ParameterIndex::new(call),
            context: ParameterIndex::new(context),
            release: ParameterIndex::new(release),
        })
    }
}

impl ParameterRole {
    fn is_byte_length(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ByteLength(name) if name == expected)
    }

    fn is_closure_context(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ClosureContext(name) if name == expected)
    }

    fn is_closure_release(&self, expected: &Identifier) -> bool {
        matches!(self, Self::ClosureRelease(name) if name == expected)
    }
}

impl Type {
    /// Creates a generated named C type.
    pub fn named(name: impl Into<String>) -> Result<Self> {
        Identifier::parse(name).map(Self::Named)
    }

    /// Creates the C ABI type for a primitive scalar.
    pub fn primitive(primitive: Primitive) -> Result<Self> {
        match primitive {
            Primitive::Bool => Ok(Self::Bool),
            Primitive::I8 => Ok(Self::Int8),
            Primitive::U8 => Ok(Self::Uint8),
            Primitive::I16 => Ok(Self::Int16),
            Primitive::U16 => Ok(Self::Uint16),
            Primitive::I32 => Ok(Self::Int32),
            Primitive::U32 => Ok(Self::Uint32),
            Primitive::I64 => Ok(Self::Int64),
            Primitive::U64 => Ok(Self::Uint64),
            Primitive::ISize => Ok(Self::SignedPointerWidth),
            Primitive::USize => Ok(Self::PointerWidth),
            Primitive::F32 => Ok(Self::Float32),
            Primitive::F64 => Ok(Self::Float64),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown primitive",
            }),
        }
    }

    fn handle_carrier(carrier: native::HandleCarrier) -> Result<Self> {
        match carrier {
            native::HandleCarrier::U64 => Ok(Self::Uint64),
            native::HandleCarrier::USize => Ok(Self::PointerWidth),
            native::HandleCarrier::CallbackHandle => Ok(Self::CallbackHandle),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "unknown native handle carrier",
            }),
        }
    }
}

impl Names {
    fn new(bindings: &Bindings<Native>) -> Result<Self> {
        bindings
            .decls()
            .iter()
            .try_fold(Self::default(), |mut names, decl| {
                names.insert(decl)?;
                Ok(names)
            })
    }

    fn insert(&mut self, decl: &Decl<Native>) -> Result<()> {
        match DeclarationRef::from(decl) {
            DeclarationRef::Record(RecordDecl::Direct(record)) => {
                self.direct_records.insert(
                    record.id(),
                    Identifier::parse(name::Spelling::new(record.name()).typedef())?,
                );
            }
            DeclarationRef::Record(RecordDecl::Encoded(_)) => {}
            DeclarationRef::Record(_) => {}
            DeclarationRef::Enum(EnumDecl::CStyle(enumeration)) => {
                self.enums.insert(
                    enumeration.id(),
                    Identifier::parse(name::Spelling::new(enumeration.name()).typedef())?,
                );
                self.c_style_enum_reprs.insert(
                    enumeration.id(),
                    Type::primitive(enumeration.repr().primitive())?,
                );
            }
            DeclarationRef::Enum(EnumDecl::Data(enumeration)) => {
                self.enums.insert(
                    enumeration.id(),
                    Identifier::parse(name::Spelling::new(enumeration.name()).typedef())?,
                );
            }
            DeclarationRef::Enum(enumeration) => {
                self.enums.insert(
                    enumeration.id(),
                    Identifier::parse(name::Spelling::new(enumeration.name()).typedef())?,
                );
            }
            DeclarationRef::Class(class) => {
                self.classes.insert(
                    class.id(),
                    Identifier::parse(name::Spelling::new(class.name()).typedef())?,
                );
                self.class_handles.insert(class.id(), class.handle());
            }
            DeclarationRef::Callback(callback) => {
                self.callbacks.insert(
                    callback.id(),
                    Identifier::parse(name::Spelling::new(callback.name()).typedef())?,
                );
            }
            DeclarationRef::Stream(stream) => {
                self.streams.insert(
                    stream.id(),
                    Identifier::parse(name::Spelling::new(stream.name()).typedef())?,
                );
            }
            DeclarationRef::CustomType(_) => {}
            DeclarationRef::Function(_) | DeclarationRef::Constant(_) => {}
        }
        Ok(())
    }

    fn record(&self, id: RecordId) -> Result<Identifier> {
        self.direct_records
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing direct record type name",
            })
    }

    fn enumeration(&self, id: EnumId) -> Result<Identifier> {
        self.enums
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing enum type name",
            })
    }

    fn callback(&self, id: CallbackId) -> Result<Identifier> {
        self.callbacks
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing callback type name",
            })
    }

    fn class_handle(&self, id: ClassId) -> Result<native::HandleCarrier> {
        self.class_handles
            .get(&id)
            .copied()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing class handle carrier",
            })
    }

    fn direct_value(&self, ty: &DirectValueType) -> Result<Type> {
        match ty {
            DirectValueType::Primitive(primitive) => Type::primitive(*primitive),
            DirectValueType::Record(record) => self.record(*record).map(Type::DirectRecord),
            DirectValueType::Enum(enumeration) => Ok(Type::CStyleEnum {
                name: self.enumeration(*enumeration)?,
                repr: Box::new(self.c_style_enum_repr(*enumeration)?),
            }),
            _ => Err(Error::UnexpectedBindingShape {
                layer: C_BRIDGE_LAYER,
                shape: "direct value type",
            }),
        }
    }

    fn c_style_enum_repr(&self, id: EnumId) -> Result<Type> {
        self.c_style_enum_reprs
            .get(&id)
            .cloned()
            .ok_or(Error::BrokenBridgeContract {
                bridge: C_BRIDGE_CONTRACT,
                invariant: "missing C-style enum representation",
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
                        .chain(params.into_iter().map(|parameter| parameter.ty))
                        .chain(
                            self.callback_return_params(returns)?
                                .into_iter()
                                .map(|parameter| parameter.ty),
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

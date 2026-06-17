use std::collections::BTreeMap;

use boltffi_binding::{
    Bindings, CStyleEnumDecl, CallableDecl, CallbackDecl, ClassDecl, ClassId, ConstantDecl,
    ConstantValueDecl, CustomTypeId, Decl, DeclarationRef, DirectRecordDecl, Direction, EnumDecl,
    EnumId, ErrorDecl, ExportedCallable, ExportedMethodDecl, ImportedCallable, ImportedMethodDecl,
    InitializerDecl, IntoRust, Native, NativeSymbol, OutOfRust, ParamDecl, ParamDirection,
    ParamPlan, Primitive, RecordDecl, RecordId, ReturnPlan, RustBody, StreamDecl, StreamItemPlan,
    TypeRef, VTableSlot, native,
};

use crate::core::{
    BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result, contract::sealed,
};

use super::name;

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
    name: String,
    fields: Vec<Field>,
}

/// A C field declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Field {
    name: String,
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
    name: String,
    repr: Type,
    variants: Vec<EnumVariant>,
}

/// A C enum variant constant.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct EnumVariant {
    name: String,
    value: i128,
}

/// A native callback vtable declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Callback {
    vtable: Record,
    register: Function,
    create_handle: Function,
}

/// A C function declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Function {
    name: String,
    params: Vec<Parameter>,
    returns: Type,
}

/// A C function parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Parameter {
    name: String,
    ty: Type,
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
    Named(String),
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
    direct_records: BTreeMap<RecordId, String>,
    enums: BTreeMap<EnumId, String>,
    classes: BTreeMap<boltffi_binding::ClassId, String>,
    class_handles: BTreeMap<ClassId, native::HandleCarrier>,
    callbacks: BTreeMap<boltffi_binding::CallbackId, String>,
    streams: BTreeMap<boltffi_binding::StreamId, String>,
    customs: BTreeMap<CustomTypeId, TypeRef>,
}

#[derive(Clone, Debug)]
struct Signature<'names> {
    names: &'names Names,
    receiver: Vec<Parameter>,
}

#[derive(Clone, Copy, Debug)]
struct PollHandleSymbols<'protocol> {
    start: &'protocol NativeSymbol,
    poll: &'protocol NativeSymbol,
    complete: &'protocol NativeSymbol,
    cancel: &'protocol NativeSymbol,
    free: &'protocol NativeSymbol,
    panic_message: &'protocol NativeSymbol,
}

impl<'protocol> PollHandleSymbols<'protocol> {
    fn new(
        start: &'protocol NativeSymbol,
        poll: &'protocol NativeSymbol,
        complete: &'protocol NativeSymbol,
        cancel: &'protocol NativeSymbol,
        free: &'protocol NativeSymbol,
        panic_message: &'protocol NativeSymbol,
    ) -> Self {
        Self {
            start,
            poll,
            complete,
            cancel,
            free,
            panic_message,
        }
    }
}

impl CBridgeContract {
    /// Builds the C ABI contract for native bindings.
    pub fn from_bindings(bindings: &Bindings<Native>, header_path: FilePath) -> Result<Self> {
        let names = Names::new(bindings);
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
                            return Err(Error::UnsupportedCAbi {
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
            support: SupportFunctions::new(),
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
        &self.name
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
                Ok(Field::new(
                    name::Field::new(field.key()).spelling()?,
                    names.type_ref(field.ty())?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { name, fields })
    }
}

impl Field {
    /// Returns the field name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the field type.
    pub fn ty(&self) -> &Type {
        &self.ty
    }
}

impl Field {
    fn new(name: impl Into<String>, ty: Type) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

impl SupportFunctions {
    /// Creates the C ABI support function set.
    pub fn new() -> Self {
        Self {
            functions: vec![
                Function::new(
                    "boltffi_free_string",
                    vec![Parameter::new("string", Type::String)],
                    Type::Void,
                ),
                Function::new(
                    "boltffi_free_buf",
                    vec![Parameter::new("buf", Type::Buffer)],
                    Type::Void,
                ),
                Function::new(
                    "boltffi_last_error_message",
                    vec![Parameter::new(
                        "out",
                        Type::MutPointer(Box::new(Type::String)),
                    )],
                    Type::Status,
                ),
                Function::new("boltffi_clear_last_error", Vec::new(), Type::Void),
            ],
        }
    }

    /// Returns C ABI support functions.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }
}

impl Default for SupportFunctions {
    fn default() -> Self {
        Self::new()
    }
}

impl Enum {
    /// Returns the C typedef name.
    pub fn name(&self) -> &str {
        &self.name
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
                    .collect(),
            }),
            _ => Err(Error::UnsupportedCAbi {
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
                .collect(),
        })
    }
}

impl EnumVariant {
    /// Returns the C constant name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the integer constant value.
    pub const fn value(&self) -> i128 {
        self.value
    }
}

impl EnumVariant {
    fn new(name: impl Into<String>, value: i128) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}

impl Callback {
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
        let vtable_name = format!("{}VTable", names.callback(callback.id())?);
        let vtable = callback.protocol().vtable();
        let free = Field::new(
            vtable.free_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Void),
                params: vec![Type::Uint64],
            },
        );
        let clone = Field::new(
            vtable.clone_slot().as_str(),
            Type::FunctionPointer {
                returns: Box::new(Type::Uint64),
                params: vec![Type::Uint64],
            },
        );
        let methods = vtable
            .methods()
            .iter()
            .map(|method| callback_field(method, names))
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
            )],
            Type::Void,
        );
        let create_handle = Function::new(
            callback.protocol().create_handle().name().as_str(),
            vec![Parameter::new("handle", Type::Uint64)],
            Type::CallbackHandle,
        );
        Ok(Self {
            vtable,
            register,
            create_handle,
        })
    }
}

impl Function {
    /// Returns the C symbol name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parameters in C ABI order.
    pub fn params(&self) -> &[Parameter] {
        &self.params
    }

    /// Returns the C return type.
    pub fn returns(&self) -> &Type {
        &self.returns
    }
}

impl Function {
    fn from_decl(decl: DeclarationRef<'_, Native>, names: &Names) -> Result<Vec<Self>> {
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
                vec![Parameter::new(
                    "receiver",
                    Type::Named(names.record(record.id())?),
                )],
            ),
            RecordDecl::Encoded(record) => (
                record.initializers(),
                record.methods(),
                encoded_receiver("receiver"),
            ),
            _ => {
                return Err(Error::UnsupportedCAbi {
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
                vec![Parameter::new(
                    "receiver",
                    Type::Named(names.enumeration(enumeration.id())?),
                )],
            ),
            EnumDecl::Data(enumeration) => (
                enumeration.initializers(),
                enumeration.methods(),
                encoded_receiver("receiver"),
            ),
            _ => {
                return Err(Error::UnsupportedCAbi {
                    shape: "unknown enum declaration",
                });
            }
        };
        Self::associated_functions(initializers, methods, receiver, names)
    }

    fn class_functions(class: &ClassDecl<Native>, names: &Names) -> Result<Vec<Self>> {
        let receiver = vec![Parameter::new(
            "receiver",
            Type::handle_carrier(class.handle())?,
        )];
        let release = Self::new(
            class.release().name().as_str(),
            vec![Parameter::new(
                "handle",
                Type::handle_carrier(class.handle())?,
            )],
            Type::Void,
        );
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
            _ => Err(Error::UnsupportedCAbi {
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
                    .map(|ty| Parameter::new("receiver", ty))
            })
            .transpose()?
            .into_iter()
            .collect();
        let pop_batch = match stream.item() {
            StreamItemPlan::Direct { ty, .. } => Self::new(
                protocol.pop_batch().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone()),
                    Parameter::new(
                        "output_ptr",
                        Type::MutPointer(Box::new(names.type_ref(ty)?)),
                    ),
                    Parameter::new("output_capacity", Type::PointerWidth),
                ],
                Type::PointerWidth,
            ),
            StreamItemPlan::Encoded { shape, .. } => Self::new(
                protocol.pop_batch().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone()),
                    Parameter::new("max_count", Type::PointerWidth),
                ],
                Signature::new(names, Vec::new()).encoded_return(*shape)?,
            ),
            _ => {
                return Err(Error::UnsupportedCAbi {
                    shape: "unknown stream item plan",
                });
            }
        };
        Ok(vec![
            Self::new(
                protocol.subscribe().name().as_str(),
                subscribe_params,
                subscription.clone(),
            ),
            pop_batch,
            Self::new(
                protocol.wait().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone()),
                    Parameter::new("timeout_milliseconds", Type::Uint32),
                ],
                Type::WaitResult,
            ),
            Self::new(
                protocol.poll().name().as_str(),
                vec![
                    Parameter::new("subscription", subscription.clone()),
                    Parameter::new("callback_data", Type::Uint64),
                    Parameter::new(
                        "callback",
                        Type::FunctionPointer {
                            returns: Box::new(Type::Void),
                            params: vec![Type::Uint64, Type::StreamPollResult],
                        },
                    ),
                ],
                Type::Void,
            ),
            Self::new(
                protocol.unsubscribe().name().as_str(),
                vec![Parameter::new("subscription", subscription.clone())],
                Type::Void,
            ),
            Self::new(
                protocol.free().name().as_str(),
                vec![Parameter::new("subscription", subscription)],
                Type::Void,
            ),
        ])
    }

    fn associated_functions(
        initializers: &[InitializerDecl<Native>],
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        receiver: Vec<Parameter>,
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
                    .map(|_| receiver.clone())
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

    fn new(name: impl Into<String>, params: Vec<Parameter>, returns: Type) -> Self {
        Self {
            name: name.into(),
            params,
            returns,
        }
    }
}

impl Parameter {
    /// Returns the parameter name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parameter type.
    pub fn ty(&self) -> &Type {
        &self.ty
    }
}

impl Parameter {
    fn new(name: impl Into<String>, ty: Type) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

impl Type {
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
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown primitive",
            }),
        }
    }

    fn handle_carrier(carrier: native::HandleCarrier) -> Result<Self> {
        match carrier {
            native::HandleCarrier::U64 => Ok(Self::Uint64),
            native::HandleCarrier::USize => Ok(Self::PointerWidth),
            native::HandleCarrier::CallbackHandle => Ok(Self::CallbackHandle),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown native handle carrier",
            }),
        }
    }
}

impl Names {
    fn new(bindings: &Bindings<Native>) -> Self {
        bindings
            .decls()
            .iter()
            .fold(Self::default(), |mut names, decl| {
                names.insert(decl);
                names
            })
    }

    fn insert(&mut self, decl: &Decl<Native>) {
        match DeclarationRef::from(decl) {
            DeclarationRef::Record(RecordDecl::Direct(record)) => {
                self.direct_records
                    .insert(record.id(), name::Spelling::new(record.name()).typedef());
            }
            DeclarationRef::Record(RecordDecl::Encoded(_)) => {}
            DeclarationRef::Record(_) => {}
            DeclarationRef::Enum(enumeration) => {
                self.enums.insert(
                    enumeration.id(),
                    name::Spelling::new(enumeration.name()).typedef(),
                );
            }
            DeclarationRef::Class(class) => {
                self.classes
                    .insert(class.id(), name::Spelling::new(class.name()).typedef());
                self.class_handles.insert(class.id(), class.handle());
            }
            DeclarationRef::Callback(callback) => {
                self.callbacks.insert(
                    callback.id(),
                    name::Spelling::new(callback.name()).typedef(),
                );
            }
            DeclarationRef::Stream(stream) => {
                self.streams
                    .insert(stream.id(), name::Spelling::new(stream.name()).typedef());
            }
            DeclarationRef::CustomType(custom) => {
                self.customs
                    .insert(custom.id(), custom.representation().clone());
            }
            DeclarationRef::Function(_) | DeclarationRef::Constant(_) => {}
        }
    }

    fn record(&self, id: RecordId) -> Result<String> {
        self.direct_records
            .get(&id)
            .cloned()
            .ok_or(Error::UnsupportedCAbi {
                shape: "missing direct record type name",
            })
    }

    fn enumeration(&self, id: EnumId) -> Result<String> {
        self.enums.get(&id).cloned().ok_or(Error::UnsupportedCAbi {
            shape: "missing enum type name",
        })
    }

    fn callback(&self, id: boltffi_binding::CallbackId) -> Result<String> {
        self.callbacks
            .get(&id)
            .cloned()
            .ok_or(Error::UnsupportedCAbi {
                shape: "missing callback type name",
            })
    }

    fn class_handle(&self, id: ClassId) -> Result<native::HandleCarrier> {
        self.class_handles
            .get(&id)
            .copied()
            .ok_or(Error::UnsupportedCAbi {
                shape: "missing class handle carrier",
            })
    }

    fn type_ref(&self, ty: &TypeRef) -> Result<Type> {
        match ty {
            TypeRef::Primitive(primitive) => Type::primitive(*primitive),
            TypeRef::String
            | TypeRef::Bytes
            | TypeRef::Builtin(_)
            | TypeRef::Sequence(_)
            | TypeRef::Optional(_) => Ok(Type::Buffer),
            TypeRef::Record(id) => self.record(*id).map(Type::Named),
            TypeRef::Enum(id) => self.enumeration(*id).map(Type::Named),
            TypeRef::Class(id) => {
                self.classes
                    .get(id)
                    .cloned()
                    .map(Type::Named)
                    .ok_or(Error::UnsupportedCAbi {
                        shape: "missing class type name",
                    })
            }
            TypeRef::Callback(_) => Ok(Type::CallbackHandle),
            TypeRef::Custom(id) => self.custom_type(*id),
            TypeRef::Tuple(_) | TypeRef::Result { .. } | TypeRef::Map { .. } => {
                Err(Error::UnsupportedCAbi {
                    shape: "direct tuple, result, or map C type",
                })
            }
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown C type reference",
            }),
        }
    }

    fn custom_type(&self, id: CustomTypeId) -> Result<Type> {
        self.customs
            .get(&id)
            .ok_or(Error::UnsupportedCAbi {
                shape: "missing custom type representation",
            })
            .and_then(|ty| self.type_ref(ty))
    }
}

impl<'names> Signature<'names> {
    fn new(names: &'names Names, receiver: impl IntoIterator<Item = Parameter>) -> Self {
        Self {
            names,
            receiver: receiver.into_iter().collect(),
        }
    }

    fn exported(
        self,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
    ) -> Result<Vec<Function>> {
        match callable.execution() {
            boltffi_binding::ExecutionDecl::Synchronous(_) => self
                .synchronous(symbol.name().as_str(), callable)
                .map(|function| vec![function]),
            boltffi_binding::ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
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
            boltffi_binding::ExecutionDecl::Asynchronous(_) => Err(Error::UnsupportedCAbi {
                shape: "native async protocol",
            }),
            _ => Err(Error::UnsupportedCAbi {
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
        Ok(Function::new(name, params, returns))
    }

    fn async_poll_handle(
        &self,
        callable: &ExportedCallable<Native>,
        symbols: PollHandleSymbols<'_>,
    ) -> Result<Vec<Function>> {
        let start = Function::new(
            symbols.start.name().as_str(),
            self.receiver
                .clone()
                .into_iter()
                .chain(self.exported_params(callable.params())?)
                .collect(),
            Type::FutureHandle,
        );
        let complete_params = std::iter::once(Parameter::new("handle", Type::FutureHandle))
            .chain([Parameter::new(
                "out_status",
                Type::MutPointer(Box::new(Type::Status)),
            )])
            .chain(self.return_params(callable.returns().plan())?)
            .collect();
        Ok(vec![
            start,
            Function::new(
                symbols.poll.name().as_str(),
                vec![
                    Parameter::new("handle", Type::FutureHandle),
                    Parameter::new("callback_data", Type::Uint64),
                    Parameter::new(
                        "callback",
                        Type::FunctionPointer {
                            returns: Box::new(Type::Void),
                            params: vec![Type::Uint64, Type::Int8],
                        },
                    ),
                ],
                Type::Void,
            ),
            Function::new(
                symbols.complete.name().as_str(),
                complete_params,
                self.async_complete_return(callable.returns().plan(), callable.error())?,
            ),
            Function::new(
                symbols.panic_message.name().as_str(),
                vec![Parameter::new("handle", Type::FutureHandle)],
                Type::Buffer,
            ),
            Function::new(
                symbols.cancel.name().as_str(),
                vec![Parameter::new("handle", Type::FutureHandle)],
                Type::Void,
            ),
            Function::new(
                symbols.free.name().as_str(),
                vec![Parameter::new("handle", Type::FutureHandle)],
                Type::Void,
            ),
        ])
    }

    fn exported_params(&self, params: &[ParamDecl<Native, IntoRust>]) -> Result<Vec<Parameter>> {
        params
            .iter()
            .map(|param| {
                let name = name::Spelling::new(param.name()).parameter();
                match param.payload() {
                    boltffi_binding::IncomingParam::Value(plan) => self.value_param(&name, plan),
                    boltffi_binding::IncomingParam::Closure(closure) => {
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
                    boltffi_binding::OutgoingParam::Value(plan) => self.value_param(&name, plan),
                    boltffi_binding::OutgoingParam::Closure(closure) => {
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
    {
        match plan {
            ParamPlan::Direct { ty, .. } => {
                Ok(vec![Parameter::new(name, self.names.type_ref(ty)?)])
            }
            ParamPlan::Encoded {
                shape: native::BufferShape::Slice,
                ..
            } => Ok(vec![
                Parameter::new(
                    format!("{name}_ptr"),
                    Type::ConstPointer(Box::new(Type::Uint8)),
                ),
                Parameter::new(format!("{name}_len"), Type::PointerWidth),
            ]),
            ParamPlan::Encoded { .. } => Err(Error::UnsupportedCAbi {
                shape: "native encoded parameter shape",
            }),
            ParamPlan::Handle { carrier, .. } => {
                Ok(vec![Parameter::new(name, Type::handle_carrier(*carrier)?)])
            }
            ParamPlan::ScalarOption { .. } => Ok(vec![
                Parameter::new(
                    format!("{name}_ptr"),
                    Type::ConstPointer(Box::new(Type::Uint8)),
                ),
                Parameter::new(format!("{name}_len"), Type::PointerWidth),
            ]),
            ParamPlan::DirectVec { element } => self.direct_vec_param(name, element),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown parameter plan",
            }),
        }
    }

    fn direct_vec_param(&self, name: &str, element: &TypeRef) -> Result<Vec<Parameter>> {
        match element {
            TypeRef::Primitive(_) => Ok(vec![
                Parameter::new(
                    format!("{name}_ptr"),
                    Type::ConstPointer(Box::new(self.names.type_ref(element)?)),
                ),
                Parameter::new(format!("{name}_len"), Type::PointerWidth),
            ]),
            TypeRef::Record(_) | TypeRef::Enum(_) => Ok(vec![
                Parameter::new(
                    format!("{name}_ptr"),
                    Type::ConstPointer(Box::new(Type::Uint8)),
                ),
                Parameter::new(format!("{name}_byte_len"), Type::PointerWidth),
            ]),
            _ => Err(Error::UnsupportedCAbi {
                shape: "direct vector element",
            }),
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
            Parameter::new(
                format!("{name}_call"),
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
            ),
            Parameter::new(
                format!("{name}_context"),
                Type::MutPointer(Box::new(Type::Void)),
            ),
            Parameter::new(
                format!("{name}_release"),
                Type::FunctionPointer {
                    returns: Box::new(Type::Void),
                    params: vec![Type::MutPointer(Box::new(Type::Void))],
                },
            ),
        ])
    }

    fn return_params<D>(&self, plan: &ReturnPlan<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match plan {
            ReturnPlan::DirectViaOutPointer { ty } => Ok(vec![Parameter::new(
                "return_out",
                Type::MutPointer(Box::new(self.names.type_ref(ty)?)),
            )]),
            ReturnPlan::EncodedViaOutPointer { shape, .. } => {
                self.encoded_out("return_out", *shape)
            }
            ReturnPlan::HandleViaOutPointer { carrier, .. } => Ok(vec![Parameter::new(
                "return_out",
                Type::MutPointer(Box::new(Type::handle_carrier(*carrier)?)),
            )]),
            ReturnPlan::ClosureViaOutPointer(_) => Ok(vec![Parameter::new(
                "return_out",
                Type::MutPointer(Box::new(Type::Void)),
            )]),
            ReturnPlan::Void
            | ReturnPlan::DirectViaReturnSlot { .. }
            | ReturnPlan::EncodedViaReturnSlot { .. }
            | ReturnPlan::HandleViaReturnSlot { .. }
            | ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. } => Ok(Vec::new()),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown return plan",
            }),
        }
    }

    fn error_params<D>(&self, error: &ErrorDecl<Native, D>) -> Result<Vec<Parameter>>
    where
        D: Direction,
    {
        match error {
            ErrorDecl::StatusViaOutPointer { .. } => Ok(vec![Parameter::new(
                "error_out",
                Type::MutPointer(Box::new(Type::Status)),
            )]),
            ErrorDecl::EncodedViaOutPointer { shape, .. } => self.encoded_out("error_out", *shape),
            ErrorDecl::None(_)
            | ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::EncodedViaReturnSlot { .. } => Ok(Vec::new()),
            _ => Err(Error::UnsupportedCAbi {
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
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown error declaration",
            }),
        }
    }

    fn return_slot_type<D>(&self, plan: &ReturnPlan<Native, D>) -> Result<Type>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match plan {
            ReturnPlan::Void => Ok(Type::Status),
            ReturnPlan::DirectViaReturnSlot { ty } => self.names.type_ref(ty),
            ReturnPlan::EncodedViaReturnSlot { shape, .. } => self.encoded_return(*shape),
            ReturnPlan::HandleViaReturnSlot { carrier, .. } => Type::handle_carrier(*carrier),
            ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. } => Ok(Type::Buffer),
            ReturnPlan::DirectViaOutPointer { .. }
            | ReturnPlan::EncodedViaOutPointer { .. }
            | ReturnPlan::HandleViaOutPointer { .. }
            | ReturnPlan::ClosureViaOutPointer(_) => Ok(Type::Status),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown return plan",
            }),
        }
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
            ErrorDecl::None(_) if matches!(plan, ReturnPlan::Void) => Ok(Type::Void),
            ErrorDecl::None(_) => self.return_slot_type(plan),
            _ => Err(Error::UnsupportedCAbi {
                shape: "async error channel",
            }),
        }
    }

    fn encoded_return(&self, shape: native::BufferShape) -> Result<Type> {
        match shape {
            native::BufferShape::Buffer => Ok(Type::Buffer),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnsupportedCAbi {
                    shape: "native encoded return shape",
                })
            }
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown native encoded return shape",
            }),
        }
    }

    fn encoded_out(&self, name: &str, shape: native::BufferShape) -> Result<Vec<Parameter>> {
        match shape {
            native::BufferShape::Buffer => Ok(vec![Parameter::new(
                name,
                Type::MutPointer(Box::new(Type::Buffer)),
            )]),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnsupportedCAbi {
                    shape: "native encoded out-pointer shape",
                })
            }
            _ => Err(Error::UnsupportedCAbi {
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
        match (plan, error) {
            (ReturnPlan::Void, ErrorDecl::None(_)) => Ok(Type::Void),
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
            _ => Err(Error::UnsupportedCAbi {
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
        match plan {
            ReturnPlan::Void => Ok(None),
            ReturnPlan::DirectViaReturnSlot { ty } => Ok(Some(self.names.type_ref(ty)?)),
            ReturnPlan::EncodedViaReturnSlot { shape, .. } => {
                Ok(Some(self.encoded_return(*shape)?))
            }
            ReturnPlan::HandleViaReturnSlot { carrier, .. } => {
                Ok(Some(Type::handle_carrier(*carrier)?))
            }
            ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. } => Ok(Some(Type::Buffer)),
            ReturnPlan::ClosureViaOutPointer(_) => Err(Error::UnsupportedCAbi {
                shape: "async callback closure return",
            }),
            ReturnPlan::DirectViaOutPointer { .. }
            | ReturnPlan::EncodedViaOutPointer { .. }
            | ReturnPlan::HandleViaOutPointer { .. } => Err(Error::UnsupportedCAbi {
                shape: "infallible async callback out-pointer return",
            }),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown infallible async callback return",
            }),
        }
    }

    fn validate_fallible_async_callback_success<D>(
        &self,
        plan: &ReturnPlan<Native, D>,
    ) -> Result<()>
    where
        D: Direction,
        D::Opposite: ParamDirection<Native>,
    {
        match plan {
            ReturnPlan::Void
            | ReturnPlan::DirectViaOutPointer { .. }
            | ReturnPlan::EncodedViaOutPointer { .. }
            | ReturnPlan::HandleViaOutPointer { .. } => Ok(()),
            ReturnPlan::ClosureViaOutPointer(_) => Err(Error::UnsupportedCAbi {
                shape: "async callback closure success",
            }),
            ReturnPlan::DirectViaReturnSlot { .. }
            | ReturnPlan::EncodedViaReturnSlot { .. }
            | ReturnPlan::HandleViaReturnSlot { .. }
            | ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. } => Err(Error::UnsupportedCAbi {
                shape: "fallible async callback success slot",
            }),
            _ => Err(Error::UnsupportedCAbi {
                shape: "unknown fallible async callback success",
            }),
        }
    }
}

fn callback_field(method: &ImportedMethodDecl<Native, VTableSlot>, names: &Names) -> Result<Field> {
    let signature = Signature::new(names, Vec::new());
    if matches!(
        method.callable().execution(),
        boltffi_binding::ExecutionDecl::Asynchronous(_)
    ) {
        return async_callback_field(method, &signature);
    }
    let return_params = signature.callback_return_params(method.callable().returns().plan())?;
    let method_params = signature.imported_params(method.callable().params())?;
    let params = std::iter::once(Type::Uint64)
        .chain(return_params.into_iter().map(|parameter| parameter.ty))
        .chain(method_params.into_iter().map(|parameter| parameter.ty))
        .collect();
    Ok(Field::new(
        method.target().as_str(),
        Type::FunctionPointer {
            returns: Box::new(signature.callback_return_type(
                method.callable().returns().plan(),
                method.callable().error(),
            )?),
            params,
        },
    ))
}

fn async_callback_field(
    method: &ImportedMethodDecl<Native, VTableSlot>,
    signature: &Signature<'_>,
) -> Result<Field> {
    let method_params = signature.imported_params(method.callable().params())?;
    let completion = signature.async_completion(
        method.callable().returns().plan(),
        method.callable().error(),
    )?;
    let params = std::iter::once(Type::Uint64)
        .chain(method_params.into_iter().map(|parameter| parameter.ty))
        .chain([completion, Type::MutPointer(Box::new(Type::Void))])
        .collect();
    Ok(Field::new(
        method.target().as_str(),
        Type::FunctionPointer {
            returns: Box::new(Type::Void),
            params,
        },
    ))
}

fn encoded_receiver(name: &str) -> Vec<Parameter> {
    vec![
        Parameter::new(
            format!("{name}_ptr"),
            Type::ConstPointer(Box::new(Type::Uint8)),
        ),
        Parameter::new(format!("{name}_len"), Type::PointerWidth),
    ]
}

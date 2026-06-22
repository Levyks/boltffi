use std::collections::BTreeMap;

use boltffi_binding::{
    Bindings, CallbackDecl, CallbackId, DeclarationRef, EnumDecl, EnumId, ExecutionDecl,
    ImportedMethodDecl, Native, RecordDecl, RecordId, VTableSlot,
};

use crate::core::{
    BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result, contract::sealed,
};

use super::function::Signature;
use super::names::Names;
use super::{
    C_BRIDGE_LAYER, Enum, Field, Function, Identifier, Parameter, Record, SupportFunctions, Type,
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

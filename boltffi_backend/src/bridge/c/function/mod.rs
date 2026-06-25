mod poll;
mod receiver;
mod signature;

pub use signature::Signature;

use boltffi_binding::{
    ClassDecl, ConstantDecl, ConstantValueDecl, DeclarationRef, EnumDecl, ExportedCallable,
    ExportedMethodDecl, InitializerDecl, Native, NativeSymbol, RecordDecl,
};

use crate::core::{Error, Result};

use self::receiver::ReceiverAbi;
use super::{
    C_BRIDGE_LAYER, Identifier, Parameter, ParameterGroup, ParameterIndex, Type, names::Names,
};

/// Meaning of the C ABI return slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ReturnChannel {
    /// The return slot carries the callable success value.
    Value,
    /// The return slot carries an encoded error payload.
    EncodedError,
}

/// A C function declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Function {
    name: Identifier,
    params: Vec<Parameter>,
    parameter_groups: Vec<ParameterGroup>,
    returns: Type,
    return_channel: ReturnChannel,
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

    /// Returns the semantic channel carried by the C return slot.
    pub fn return_channel(&self) -> ReturnChannel {
        self.return_channel
    }
}

impl Function {
    /// Builds C ABI function declarations exposed by one lowered declaration.
    pub fn from_decl<'decl>(
        decl: DeclarationRef<'decl, Native>,
        names: &Names,
    ) -> Result<Vec<Self>> {
        match decl {
            DeclarationRef::Function(function) => {
                Self::exported(function.symbol(), function.callable(), Vec::new(), names)
            }
            DeclarationRef::Record(record) => Self::record_functions(record, names),
            DeclarationRef::Enum(enumeration) => Self::enum_functions(enumeration, names),
            DeclarationRef::Class(class) => Self::class_functions(class, names),
            DeclarationRef::Constant(constant) => Self::constant_functions(constant, names),
            DeclarationRef::Stream(_) => Ok(Vec::new()),
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
        Self::with_return_channel(name, params, returns, ReturnChannel::Value)
    }

    /// Creates a C function declaration with an explicit return slot channel.
    pub fn with_return_channel(
        name: impl Into<String>,
        params: Vec<Parameter>,
        returns: Type,
        return_channel: ReturnChannel,
    ) -> Result<Self> {
        let parameter_groups = ParameterGroup::from_params(&params)?;
        Ok(Self {
            name: Identifier::parse(name)?,
            params,
            parameter_groups,
            returns,
            return_channel,
        })
    }
}

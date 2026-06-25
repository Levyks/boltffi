use std::collections::{HashMap, HashSet};

use boltffi_binding::{
    CallbackDecl, ClassDecl, ExportedCallable, FunctionDecl, Native, NativeSymbol,
};

use crate::{
    bridge::{
        c::Identifier as CIdentifier,
        jni::{
            CallbackCompletionInvoker, CallbackCompletionPayload, CallbackCompletionPayloadValue,
            CallbackHandleLifecycle, CallbackHandleMethod, CallbackRegistration, JniBridgeContract,
            NativeMethod, NativeParameter, SuccessOutValue, SuccessOutWriter,
        },
    },
    core::{Error, Result},
    target::kotlin::{
        render::type_name::KotlinType,
        syntax::{ArgumentList, Expression, Identifier, TypeName},
    },
};

const JNI_BRIDGE: &str = "jni";

pub struct NativeMethods<'bridge> {
    methods: HashMap<&'bridge str, &'bridge NativeMethod>,
    callbacks: HashMap<&'bridge str, &'bridge CallbackRegistration>,
    success_out_writers: &'bridge [SuccessOutWriter],
    callback_completions: &'bridge [CallbackCompletionInvoker],
    callback_handle_lifecycle: Option<&'bridge CallbackHandleLifecycle>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunction {
    name: Identifier,
    parameters: Vec<NativeFunctionParameter>,
    returns: TypeName,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFunctionParameter {
    name: Identifier,
    ty: TypeName,
}

impl<'bridge> NativeMethods<'bridge> {
    pub fn new(bridge: &'bridge JniBridgeContract) -> Self {
        Self {
            methods: bridge
                .methods()
                .iter()
                .map(|method| (method.c_function().name(), method))
                .collect(),
            callbacks: bridge
                .callbacks()
                .iter()
                .map(|callback| (callback.register().as_str(), callback))
                .collect(),
            success_out_writers: bridge.success_out_writers(),
            callback_completions: bridge.callback_completions(),
            callback_handle_lifecycle: bridge.callback_handle_lifecycle(),
        }
    }

    pub fn function(&self, decl: &FunctionDecl<Native>) -> Result<Vec<NativeFunction>> {
        self.exported(decl.symbol(), decl.callable())
    }

    pub fn class(&self, decl: &ClassDecl<Native>) -> Result<Vec<NativeFunction>> {
        std::iter::once(decl.release())
            .map(|symbol| self.symbol(symbol).map(|function| vec![function]))
            .chain(
                decl.initializers()
                    .iter()
                    .map(|initializer| self.exported(initializer.symbol(), initializer.callable())),
            )
            .chain(
                decl.methods()
                    .iter()
                    .map(|method| self.exported(method.target(), method.callable())),
            )
            .collect::<Result<Vec<_>>>()
            .map(|functions| {
                functions
                    .into_iter()
                    .flatten()
                    .collect::<Vec<NativeFunction>>()
            })
    }

    pub fn callback(&self, decl: &CallbackDecl<Native>) -> Result<Vec<NativeFunction>> {
        self.callbacks
            .get(decl.protocol().register().name().as_str())
            .copied()
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback declaration has no JNI registration",
            })?
            .handle_methods()
            .iter()
            .map(NativeFunction::from_callback_handle_method)
            .collect()
    }

    pub fn callback_handle_lifecycle(&self) -> Result<Vec<NativeFunction>> {
        self.callback_handle_lifecycle
            .map(NativeFunction::from_callback_handle_lifecycle)
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn callback_completions(&self) -> Result<Vec<NativeFunction>> {
        self.callback_completions
            .iter()
            .map(NativeFunction::from_callback_completion_invoker)
            .collect::<Result<Vec<_>>>()
            .map(|functions| functions.into_iter().flatten().collect())
    }

    pub fn success_out_writers(&self) -> Result<Vec<NativeFunction>> {
        self.success_out_writers
            .iter()
            .map(NativeFunction::from_success_out_writer)
            .collect()
    }

    fn exported(
        &self,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
    ) -> Result<Vec<NativeFunction>> {
        let mut seen = HashSet::new();
        std::iter::once(symbol)
            .chain(callable.native_symbols())
            .filter(|symbol| seen.insert(symbol.name().as_str()))
            .map(|symbol| self.symbol(symbol))
            .collect()
    }

    fn symbol(&self, symbol: &NativeSymbol) -> Result<NativeFunction> {
        self.methods
            .get(symbol.name().as_str())
            .copied()
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "declaration has no JNI native method",
            })
            .and_then(NativeFunction::from_method)
    }
}

impl NativeFunction {
    pub fn from_method(method: &NativeMethod) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(method.c_function().name())?,
            parameters: method
                .parameters()
                .iter()
                .map(NativeFunctionParameter::from_parameter)
                .collect::<Result<Vec<_>>>()?,
            returns: KotlinType::native_return(method.returns())?,
        })
    }

    pub fn from_callback_handle_method(method: &CallbackHandleMethod) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(method.method().as_str())?,
            parameters: std::iter::once(NativeFunctionParameter::callback_handle()?)
                .chain(
                    method
                        .parameters()
                        .iter()
                        .map(NativeFunctionParameter::from_parameter)
                        .collect::<Result<Vec<_>>>()?,
                )
                .collect(),
            returns: KotlinType::callback_handle_return(method)?,
        })
    }

    pub fn from_callback_handle_lifecycle(
        lifecycle: &CallbackHandleLifecycle,
    ) -> Result<Vec<Self>> {
        vec![
            Self::callback_handle_lifecycle_function(lifecycle.clone_method(), TypeName::long()),
            Self::callback_handle_lifecycle_function(lifecycle.release_method(), TypeName::unit()),
        ]
        .into_iter()
        .collect()
    }

    pub fn from_success_out_writer(writer: &SuccessOutWriter) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(writer.method().as_str())?,
            parameters: vec![
                NativeFunctionParameter {
                    name: Identifier::parse("returnOut")?,
                    ty: TypeName::long(),
                },
                NativeFunctionParameter {
                    name: Identifier::parse("value")?,
                    ty: Self::success_out_value_type(writer.value())?,
                },
            ],
            returns: TypeName::unit(),
        })
    }

    pub fn from_callback_completion_invoker(
        invoker: &CallbackCompletionInvoker,
    ) -> Result<Vec<Self>> {
        let payload = invoker.payload();
        Ok([
            Some(Self::callback_completion_function(
                invoker.success_method().as_str(),
                payload,
            )?),
            Some(Self::callback_failure_function(
                invoker.failure_method().as_str(),
            )?),
            invoker
                .error_method()
                .map(|method| Self::callback_completion_function(method.as_str(), payload))
                .transpose()?,
        ]
        .into_iter()
        .flatten()
        .collect())
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[NativeFunctionParameter] {
        &self.parameters
    }

    pub fn returns(&self) -> &TypeName {
        &self.returns
    }

    fn callback_handle_lifecycle_function(method: &CIdentifier, returns: TypeName) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(method.as_str())?,
            parameters: vec![NativeFunctionParameter::callback_handle()?],
            returns,
        })
    }

    fn success_out_value_type(value: &SuccessOutValue) -> Result<TypeName> {
        match value {
            SuccessOutValue::Scalar { jni_type, .. } => KotlinType::jni(*jni_type),
            SuccessOutValue::Bytes | SuccessOutValue::Record { .. } => {
                Ok(TypeName::byte_array(false))
            }
        }
    }

    fn callback_completion_function(
        name: &str,
        payload: Option<&CallbackCompletionPayload>,
    ) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(name)?,
            parameters: [
                NativeFunctionParameter::callback_pointer()?,
                NativeFunctionParameter::callback_context()?,
            ]
            .into_iter()
            .chain(
                payload
                    .map(NativeFunctionParameter::callback_payload)
                    .transpose()?,
            )
            .collect(),
            returns: TypeName::unit(),
        })
    }

    fn callback_failure_function(name: &str) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(name)?,
            parameters: vec![
                NativeFunctionParameter::callback_pointer()?,
                NativeFunctionParameter::callback_context()?,
            ],
            returns: TypeName::unit(),
        })
    }
}

impl NativeFunctionParameter {
    pub fn from_parameter(parameter: &NativeParameter) -> Result<Self> {
        Ok(Self {
            name: Identifier::escape(parameter.name().as_str())?,
            ty: KotlinType::native_parameter(parameter.kind())?,
        })
    }

    pub fn callback_handle() -> Result<Self> {
        Ok(Self {
            name: Identifier::parse("handle")?,
            ty: TypeName::long(),
        })
    }

    pub fn callback_pointer() -> Result<Self> {
        Ok(Self {
            name: Identifier::parse("callback")?,
            ty: TypeName::long(),
        })
    }

    pub fn callback_context() -> Result<Self> {
        Ok(Self {
            name: Identifier::parse("context")?,
            ty: TypeName::long(),
        })
    }

    pub fn callback_payload(payload: &CallbackCompletionPayload) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse("result")?,
            ty: match payload.value() {
                CallbackCompletionPayloadValue::Scalar(jni_type) => KotlinType::jni(jni_type)?,
                CallbackCompletionPayloadValue::Bytes | CallbackCompletionPayloadValue::Record => {
                    TypeName::byte_array(false)
                }
                CallbackCompletionPayloadValue::CallbackHandle => TypeName::long(),
            },
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }
}

pub struct NativeCall {
    function: Identifier,
    arguments: Vec<Expression>,
}

impl NativeCall {
    pub fn new(function: Identifier, arguments: Vec<Expression>) -> Self {
        Self {
            function,
            arguments,
        }
    }

    pub fn expression(&self) -> Expression {
        Expression::call(
            "Native",
            self.function.clone(),
            self.arguments.iter().cloned().collect::<ArgumentList>(),
        )
    }
}

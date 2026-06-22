use crate::{
    bridge::{
        c::{self, ArgumentList, Identifier, TypeFragment},
        jni::{
            CallbackArgument, CallbackBytesArgument, CallbackCParameter, CallbackRecordArgument,
            JniReturn,
        },
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI method dispatch for one callback vtable slot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackMethod {
    function: Identifier,
    method: Identifier,
    method_id: Identifier,
    signature: String,
    returns: JniReturn,
    arguments: Vec<CallbackArgument>,
}

impl CallbackMethod {
    /// Returns the generated C vtable method implementation.
    pub fn function(&self) -> &Identifier {
        &self.function
    }

    /// Returns the JVM static method name.
    pub fn method(&self) -> &Identifier {
        &self.method
    }

    /// Returns the cached JNI method id symbol.
    pub fn method_id(&self) -> &Identifier {
        &self.method_id
    }

    /// Returns the JNI method descriptor.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    /// Returns the C return type for the vtable slot implementation.
    pub fn c_return_type(&self) -> &TypeFragment {
        self.returns.c_type()
    }

    /// Returns whether the slot returns no value.
    pub fn returns_void(&self) -> bool {
        self.returns.is_void()
    }

    /// Returns the `CallStatic*Method` suffix for non-void slots.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        self.returns.call_method_suffix()
    }

    /// Returns the C value returned when dispatch fails.
    pub fn failure_value(&self) -> Option<&'static str> {
        self.returns.failure_value()
    }

    /// Returns generated C parameters.
    pub fn c_parameters(&self) -> Vec<CallbackCParameter> {
        self.arguments
            .iter()
            .flat_map(CallbackArgument::c_parameters)
            .collect()
    }

    /// Returns the arguments passed to the static JVM callback method.
    pub fn jni_arguments(&self) -> ArgumentList {
        ArgumentList::from_iter(self.arguments.iter().map(CallbackArgument::jni_argument))
    }

    /// Returns byte-array callback arguments.
    pub fn byte_arrays(&self) -> Vec<CallbackBytesArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::bytes)
            .collect()
    }

    /// Returns direct-record callback arguments.
    pub fn record_arrays(&self) -> Vec<CallbackRecordArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::record)
            .collect()
    }

    pub(in crate::bridge::jni::contract::callback) fn from_slot(
        stem: &str,
        slot: &c::CallbackSlot,
    ) -> Result<Self> {
        let Some(c::Type::Uint64) = slot.parameters().first().map(c::Parameter::ty) else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback vtable slot does not start with a uint64 handle",
            });
        };
        let returns = JniReturn::from_c_type(slot.returns())?;
        let arguments = slot
            .parameter_groups()
            .iter()
            .map(|group| CallbackArgument::from_group(slot, group))
            .collect::<Result<Vec<_>>>()?;
        let signature = format!(
            "({}){}",
            arguments
                .iter()
                .map(CallbackArgument::jni_signature)
                .collect::<Vec<_>>()
                .join(""),
            returns.signature()
        );
        Ok(Self {
            function: Identifier::parse(format!("{stem}_{}", slot.name()))?,
            method: slot.name().clone(),
            method_id: Identifier::parse(format!("g_{stem}_{}_method", slot.name()))?,
            signature,
            returns,
            arguments,
        })
    }
}

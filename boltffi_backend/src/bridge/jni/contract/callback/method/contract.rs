use crate::{
    bridge::{
        c::{self, ArgumentList, Identifier, TypeFragment},
        jni::{
            CallbackArgument, CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
            CallbackClosureReturn, CallbackCompletionArgument, CallbackDirectVectorArgument,
            CallbackHandleArgument, CallbackRecordArgument, ClosureRegistration, JvmMethodReturn,
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
    returns: JvmMethodReturn,
    c_parameters: Vec<CallbackCParameter>,
    closure_return: Option<CallbackClosureReturn>,
    arguments: Vec<CallbackArgument>,
}

impl CallbackMethod {
    /// Builds a JNI callback method from one C callback vtable slot.
    pub fn from_slot(
        stem: &str,
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        let Some(c::Type::Uint64) = slot.parameters().first().map(c::Parameter::ty) else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback vtable slot does not start with a uint64 handle",
            });
        };
        let (returns, closure_return) = Self::returns(slot, callbacks, closures)?;
        let arguments = Self::arguments(slot, callbacks, closures)?;
        let c_parameters = slot
            .parameters()
            .iter()
            .map(CallbackCParameter::from_parameter)
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
            c_parameters,
            closure_return,
            arguments,
        })
    }

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

    /// Returns whether the JVM callback method returns a byte array.
    pub fn returns_byte_array(&self) -> bool {
        self.returns.returns_byte_array()
    }

    /// Returns whether the JVM callback method returns owned encoded bytes.
    pub fn returns_bytes(&self) -> bool {
        self.returns.returns_bytes()
    }

    /// Returns whether the JVM callback method returns a direct record byte array.
    pub fn returns_record(&self) -> bool {
        self.returns.returns_record()
    }

    /// Returns whether the JVM callback method returns a callback handle token.
    pub fn returns_callback_handle(&self) -> bool {
        self.returns.returns_callback_handle()
    }

    /// Returns whether the JVM callback method returns an inline closure handle.
    pub fn returns_closure(&self) -> bool {
        self.returns.returns_closure()
    }

    /// Returns the C callback handle constructor for callback handle returns.
    pub fn callback_handle_constructor(&self) -> Option<&Identifier> {
        self.returns.callback_handle_constructor()
    }

    /// Returns the returned closure out-pointer contract.
    pub fn closure_return(&self) -> Option<&CallbackClosureReturn> {
        self.closure_return.as_ref()
    }

    /// Returns the `CallStatic*Method` suffix for non-void slots.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        self.returns.call_method_suffix()
    }

    /// Returns the C value returned when dispatch fails.
    pub fn failure_value(&self) -> Option<c::Expression> {
        self.returns.failure_value()
    }

    /// Returns generated C parameters.
    pub fn c_parameters(&self) -> Vec<CallbackCParameter> {
        self.c_parameters.clone()
    }

    /// Returns the arguments passed to the static JVM callback method.
    pub fn jni_arguments(&self) -> ArgumentList {
        ArgumentList::from_iter(
            self.arguments
                .iter()
                .flat_map(CallbackArgument::jni_arguments),
        )
    }

    /// Returns byte-array callback arguments.
    pub fn byte_arrays(&self) -> Vec<CallbackBytesArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::bytes)
            .collect()
    }

    /// Returns direct-vector callback arguments.
    pub fn direct_vectors(&self) -> Vec<CallbackDirectVectorArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::direct_vector)
            .collect()
    }

    /// Returns direct-record callback arguments.
    pub fn record_arrays(&self) -> Vec<CallbackRecordArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::record)
            .collect()
    }

    /// Returns callback-handle callback arguments.
    pub fn callback_handles(&self) -> Vec<CallbackHandleArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::callback_handle)
            .collect()
    }

    /// Returns closure-handle callback arguments.
    pub fn closure_handles(&self) -> Vec<CallbackClosureArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::closure_handle)
            .collect()
    }

    /// Returns async callback completion arguments.
    pub fn completions(&self) -> Vec<CallbackCompletionArgument<'_>> {
        self.arguments
            .iter()
            .filter_map(CallbackArgument::completion)
            .collect()
    }

    fn returns(
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<(JvmMethodReturn, Option<CallbackClosureReturn>)> {
        let closure_return = slot
            .parameter_groups()
            .iter()
            .filter_map(|group| match group {
                c::ParameterGroup::ClosureReturn(returned) => Some(returned),
                _ => None,
            })
            .map(|returned| CallbackClosureReturn::from_return(slot, returned, closures))
            .collect::<Result<Vec<_>>>()?;
        match closure_return.as_slice() {
            [] => JvmMethodReturn::from_c_type(slot.returns(), callbacks)
                .map(|returns| (returns, None)),
            [returned] if matches!(slot.returns(), c::Type::Status) => {
                Ok((JvmMethodReturn::closure_status()?, Some(returned.clone())))
            }
            [_] => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback closure return does not use FfiStatus",
            }),
            _ => Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback method has multiple closure return out-pointers",
            }),
        }
    }

    fn arguments(
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Vec<CallbackArgument>> {
        slot.parameter_groups()
            .iter()
            .filter(|group| !matches!(group, c::ParameterGroup::ClosureReturn(_)))
            .map(|group| CallbackArgument::from_group(slot, group, callbacks, closures))
            .collect()
    }
}

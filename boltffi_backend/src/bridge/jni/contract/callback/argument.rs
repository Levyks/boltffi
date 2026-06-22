use crate::{
    bridge::{
        c::{self, ArgumentList, Expression, Identifier, Literal, Statement, TypeFragment},
        jni::JniType,
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// One callback vtable argument forwarded to a JVM callback method.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackArgument {
    kind: CallbackArgumentKind,
}

/// One C ABI parameter accepted by a generated callback vtable slot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCParameter {
    name: Identifier,
    ty: TypeFragment,
    declaration: Statement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CallbackArgumentKind {
    Value {
        parameter: CallbackCParameter,
        jni_type: JniType,
    },
    Bytes {
        name: Identifier,
        pointer: CallbackCParameter,
        length: CallbackCParameter,
    },
    Record {
        array: Identifier,
        parameter: CallbackCParameter,
    },
    CallbackHandle {
        handle: Identifier,
        parameter: CallbackCParameter,
    },
    Completion {
        callback: CallbackCParameter,
        context: CallbackCParameter,
        payload: Option<TypeFragment>,
    },
}

impl CallbackArgument {
    /// Returns the C ABI parameters that carry this callback argument.
    pub fn c_parameters(&self) -> Vec<CallbackCParameter> {
        match &self.kind {
            CallbackArgumentKind::Value { parameter, .. } => vec![parameter.clone()],
            CallbackArgumentKind::Bytes {
                pointer, length, ..
            } => vec![pointer.clone(), length.clone()],
            CallbackArgumentKind::Record { parameter, .. }
            | CallbackArgumentKind::CallbackHandle { parameter, .. } => vec![parameter.clone()],
            CallbackArgumentKind::Completion {
                callback, context, ..
            } => vec![callback.clone(), context.clone()],
        }
    }

    /// Returns the JNI method descriptor segment for this argument.
    pub fn jni_signature(&self) -> &'static str {
        match &self.kind {
            CallbackArgumentKind::Value { jni_type, .. } => jni_type.signature(),
            CallbackArgumentKind::Bytes { .. } | CallbackArgumentKind::Record { .. } => "[B",
            CallbackArgumentKind::CallbackHandle { .. } => "J",
            CallbackArgumentKind::Completion { .. } => "JJ",
        }
    }

    /// Returns the expressions passed to the static JVM callback method.
    pub fn jni_arguments(&self) -> Vec<Expression> {
        match &self.kind {
            CallbackArgumentKind::Value {
                parameter,
                jni_type,
            } => vec![Expression::cast(
                jni_type.as_type_fragment(),
                Expression::identifier(parameter.name.clone()),
            )],
            CallbackArgumentKind::Bytes { name, .. } => {
                vec![Expression::identifier(name.clone())]
            }
            CallbackArgumentKind::Record { array, .. } => {
                vec![Expression::identifier(array.clone())]
            }
            CallbackArgumentKind::CallbackHandle { handle, .. } => {
                vec![Expression::identifier(handle.clone())]
            }
            CallbackArgumentKind::Completion {
                callback, context, ..
            } => {
                let jlong = TypeFragment::new("jlong");
                vec![
                    Expression::cast(jlong.clone(), Expression::identifier(callback.name.clone())),
                    Expression::cast(jlong, Expression::identifier(context.name.clone())),
                ]
            }
        }
    }

    /// Returns byte-array setup data when this argument carries borrowed bytes.
    pub fn bytes(&self) -> Option<CallbackBytesArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. } => None,
            CallbackArgumentKind::Bytes {
                name,
                pointer,
                length,
            } => Some(CallbackBytesArgument {
                name,
                pointer: &pointer.name,
                length: &length.name,
            }),
            CallbackArgumentKind::Record { .. } | CallbackArgumentKind::CallbackHandle { .. } => {
                None
            }
            CallbackArgumentKind::Completion { .. } => None,
        }
    }

    /// Returns record-array setup data when this argument carries a direct record.
    pub fn record(&self) -> Option<CallbackRecordArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. } | CallbackArgumentKind::Bytes { .. } => None,
            CallbackArgumentKind::CallbackHandle { .. } => None,
            CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::Record { array, parameter } => Some(CallbackRecordArgument {
                array,
                parameter: &parameter.name,
            }),
        }
    }

    /// Returns callback-handle setup data when this argument carries a callback handle.
    pub fn callback_handle(&self) -> Option<CallbackHandleArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::CallbackHandle { handle, parameter } => {
                Some(CallbackHandleArgument {
                    handle,
                    parameter: &parameter.name,
                })
            }
        }
    }

    /// Returns completion callback details for async callback methods.
    pub fn completion(&self) -> Option<CallbackCompletionArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. } => None,
            CallbackArgumentKind::Completion {
                callback,
                context,
                payload,
            } => Some(CallbackCompletionArgument {
                callback: &callback.name,
                failure_arguments: ArgumentList::from_iter(
                    [
                        Expression::identifier(context.name.clone()),
                        Expression::cast(
                            TypeFragment::new("FfiStatus"),
                            Expression::literal(Literal::status_failure()),
                        ),
                    ]
                    .into_iter()
                    .chain(payload.iter().map(|payload| {
                        Expression::cast(
                            payload.clone(),
                            Expression::literal(Literal::compound_zero()),
                        )
                    })),
                ),
            }),
        }
    }

    pub(in crate::bridge::jni::contract::callback) fn from_group(
        slot: &c::CallbackSlot,
        group: &c::ParameterGroup,
    ) -> Result<Self> {
        match group {
            c::ParameterGroup::Value(index) => Self::from_parameter(slot.parameter(*index)),
            c::ParameterGroup::ByteSlice(bytes) => Self::from_bytes(slot, bytes),
            c::ParameterGroup::CallbackCompletion(completion) => {
                Self::from_completion(slot, completion)
            }
            c::ParameterGroup::Continuation(_) => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "callback continuation parameter",
            }),
            c::ParameterGroup::Closure(_) => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "callback closure parameter",
            }),
        }
    }

    fn from_parameter(parameter: &c::Parameter) -> Result<Self> {
        if matches!(parameter.ty(), c::Type::CallbackHandle(_)) {
            return Ok(Self {
                kind: CallbackArgumentKind::CallbackHandle {
                    handle: Identifier::parse(format!("__boltffi_{}_handle", parameter.name()))?,
                    parameter: CallbackCParameter::from_parameter(parameter)?,
                },
            });
        }
        if matches!(parameter.ty(), c::Type::DirectRecord(_)) {
            return Ok(Self {
                kind: CallbackArgumentKind::Record {
                    array: Identifier::parse(format!("__boltffi_{}_array", parameter.name()))?,
                    parameter: CallbackCParameter::from_parameter(parameter)?,
                },
            });
        }
        Ok(Self {
            kind: CallbackArgumentKind::Value {
                parameter: CallbackCParameter::from_parameter(parameter)?,
                jni_type: JniType::from_c_type(parameter.ty())?,
            },
        })
    }

    fn from_bytes(slot: &c::CallbackSlot, bytes: &c::ByteSliceParameter) -> Result<Self> {
        Ok(Self {
            kind: CallbackArgumentKind::Bytes {
                name: Identifier::escape(bytes.name())?,
                pointer: CallbackCParameter::from_parameter(slot.parameter(bytes.pointer()))?,
                length: CallbackCParameter::from_parameter(slot.parameter(bytes.length()))?,
            },
        })
    }

    fn from_completion(
        slot: &c::CallbackSlot,
        completion: &c::CallbackCompletionParameter,
    ) -> Result<Self> {
        let callback = slot.parameter(completion.callback());
        let payload = match callback.ty() {
            c::Type::FunctionPointer { params, .. } => match params.as_slice() {
                [c::Type::MutPointer(context), c::Type::Status]
                    if matches!(context.as_ref(), c::Type::Void) =>
                {
                    None
                }
                [c::Type::MutPointer(context), c::Type::Status, payload]
                    if matches!(context.as_ref(), c::Type::Void) =>
                {
                    Some(TypeFragment::anonymous(payload)?)
                }
                _ => {
                    return Err(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant: "callback completion function pointer has unexpected parameters",
                    });
                }
            },
            _ => {
                return Err(Error::BrokenBridgeContract {
                    bridge: JNI_BRIDGE,
                    invariant: "callback completion parameter is not a function pointer",
                });
            }
        };
        Ok(Self {
            kind: CallbackArgumentKind::Completion {
                callback: CallbackCParameter::from_parameter(callback)?,
                context: CallbackCParameter::from_parameter(slot.parameter(completion.context()))?,
                payload,
            },
        })
    }
}

impl CallbackCParameter {
    /// Returns the C parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C parameter type.
    pub fn ty(&self) -> &TypeFragment {
        &self.ty
    }

    /// Returns this parameter as a C declaration.
    pub fn declaration(&self) -> &Statement {
        &self.declaration
    }

    fn from_parameter(parameter: &c::Parameter) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(parameter.name())?,
            ty: TypeFragment::anonymous(parameter.ty())?,
            declaration: TypeFragment::declaration(parameter.ty(), parameter.name())?,
        })
    }
}

/// Borrowed byte-array argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackBytesArgument<'argument> {
    name: &'argument Identifier,
    pointer: &'argument Identifier,
    length: &'argument Identifier,
}

impl CallbackBytesArgument<'_> {
    /// Returns the local JNI byte-array variable.
    pub fn name(&self) -> &Identifier {
        self.name
    }

    /// Returns the C byte pointer parameter.
    pub fn pointer(&self) -> &Identifier {
        self.pointer
    }

    /// Returns the C byte length parameter.
    pub fn length(&self) -> &Identifier {
        self.length
    }
}

/// Direct-record argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackRecordArgument<'argument> {
    array: &'argument Identifier,
    parameter: &'argument Identifier,
}

impl CallbackRecordArgument<'_> {
    /// Returns the local JNI byte-array variable.
    pub fn array(&self) -> &Identifier {
        self.array
    }

    /// Returns the C record parameter.
    pub fn parameter(&self) -> &Identifier {
        self.parameter
    }
}

/// Callback-handle argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackHandleArgument<'argument> {
    handle: &'argument Identifier,
    parameter: &'argument Identifier,
}

/// Completion callback invoked when async JVM callback dispatch fails.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionArgument<'argument> {
    callback: &'argument Identifier,
    failure_arguments: ArgumentList,
}

impl CallbackHandleArgument<'_> {
    /// Returns the local JVM callback-handle token.
    pub fn handle(&self) -> &Identifier {
        self.handle
    }

    /// Returns the C callback-handle parameter.
    pub fn parameter(&self) -> &Identifier {
        self.parameter
    }
}

impl CallbackCompletionArgument<'_> {
    /// Returns the C completion callback parameter.
    pub fn callback(&self) -> &Identifier {
        self.callback
    }

    /// Returns arguments that complete the async callback with failure.
    pub fn failure_arguments(&self) -> &ArgumentList {
        &self.failure_arguments
    }
}

use crate::{
    bridge::{
        c::{self, ArgumentList, Expression, Identifier, Literal, TypeFragment},
        jni::{
            CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
            CallbackCompletionArgument, CallbackDirectVectorArgument, CallbackHandleArgument,
            CallbackRecordArgument, ClosureRegistration, JniType,
        },
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
    DirectVector {
        array: Identifier,
        pointer: CallbackCParameter,
        length: CallbackCParameter,
        jni_type: JniType,
    },
    Record {
        array: Identifier,
        parameter: CallbackCParameter,
    },
    CallbackHandle {
        handle: Identifier,
        parameter: CallbackCParameter,
    },
    Closure {
        handle: Identifier,
        call: CallbackCParameter,
        context: CallbackCParameter,
        release: CallbackCParameter,
        handle_new: Identifier,
        handle_release: Identifier,
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
            }
            | CallbackArgumentKind::DirectVector {
                pointer, length, ..
            } => vec![pointer.clone(), length.clone()],
            CallbackArgumentKind::Record { parameter, .. }
            | CallbackArgumentKind::CallbackHandle { parameter, .. } => vec![parameter.clone()],
            CallbackArgumentKind::Closure {
                call,
                context,
                release,
                ..
            } => vec![call.clone(), context.clone(), release.clone()],
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
            CallbackArgumentKind::DirectVector { jni_type, .. } => jni_type.array_signature(),
            CallbackArgumentKind::CallbackHandle { .. } | CallbackArgumentKind::Closure { .. } => {
                "J"
            }
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
                Expression::identifier(parameter.name().clone()),
            )],
            CallbackArgumentKind::Bytes { name, .. } => {
                vec![Expression::identifier(name.clone())]
            }
            CallbackArgumentKind::DirectVector { array, .. } => {
                vec![Expression::identifier(array.clone())]
            }
            CallbackArgumentKind::Record { array, .. } => {
                vec![Expression::identifier(array.clone())]
            }
            CallbackArgumentKind::CallbackHandle { handle, .. } => {
                vec![Expression::identifier(handle.clone())]
            }
            CallbackArgumentKind::Closure { handle, .. } => {
                vec![Expression::identifier(handle.clone())]
            }
            CallbackArgumentKind::Completion {
                callback, context, ..
            } => {
                let jlong = TypeFragment::new("jlong");
                vec![
                    Expression::cast(
                        jlong.clone(),
                        Expression::identifier(callback.name().clone()),
                    ),
                    Expression::cast(jlong, Expression::identifier(context.name().clone())),
                ]
            }
        }
    }

    /// Returns byte-array setup data when this argument carries borrowed bytes.
    pub fn bytes(&self) -> Option<CallbackBytesArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. } | CallbackArgumentKind::DirectVector { .. } => None,
            CallbackArgumentKind::Bytes {
                name,
                pointer,
                length,
            } => Some(CallbackBytesArgument::new(
                name,
                pointer.name(),
                length.name(),
            )),
            CallbackArgumentKind::Record { .. } | CallbackArgumentKind::CallbackHandle { .. } => {
                None
            }
            CallbackArgumentKind::Closure { .. } => None,
            CallbackArgumentKind::Completion { .. } => None,
        }
    }

    /// Returns direct-vector setup data when this argument carries a Java array.
    pub fn direct_vector(&self) -> Option<CallbackDirectVectorArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. }
            | CallbackArgumentKind::Closure { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::DirectVector {
                array,
                pointer,
                length,
                jni_type,
            } => Some(CallbackDirectVectorArgument::new(
                array,
                pointer.name(),
                length.name(),
                *jni_type,
            )),
        }
    }

    /// Returns record-array setup data when this argument carries a direct record.
    pub fn record(&self) -> Option<CallbackRecordArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. } => None,
            CallbackArgumentKind::CallbackHandle { .. } => None,
            CallbackArgumentKind::Closure { .. } => None,
            CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::Record { array, parameter } => {
                Some(CallbackRecordArgument::new(array, parameter.name()))
            }
        }
    }

    /// Returns callback-handle setup data when this argument carries a callback handle.
    pub fn callback_handle(&self) -> Option<CallbackHandleArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::Closure { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::CallbackHandle { handle, parameter } => {
                Some(CallbackHandleArgument::new(handle, parameter.name()))
            }
        }
    }

    /// Returns closure-handle setup data when this argument carries a Rust-owned closure.
    pub fn closure_handle(&self) -> Option<CallbackClosureArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. }
            | CallbackArgumentKind::Completion { .. } => None,
            CallbackArgumentKind::Closure {
                handle,
                call,
                context,
                release,
                handle_new,
                handle_release,
            } => Some(CallbackClosureArgument::new(
                handle,
                call.name(),
                context.name(),
                release.name(),
                handle_new,
                handle_release,
            )),
        }
    }

    /// Returns completion callback details for async callback methods.
    pub fn completion(&self) -> Option<CallbackCompletionArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. }
            | CallbackArgumentKind::Bytes { .. }
            | CallbackArgumentKind::DirectVector { .. }
            | CallbackArgumentKind::Record { .. }
            | CallbackArgumentKind::CallbackHandle { .. } => None,
            CallbackArgumentKind::Closure { .. } => None,
            CallbackArgumentKind::Completion {
                callback,
                context,
                payload,
            } => Some(CallbackCompletionArgument::new(
                callback.name(),
                ArgumentList::from_iter(
                    [
                        Expression::identifier(context.name().clone()),
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
            )),
        }
    }

    pub(in crate::bridge::jni::contract::callback) fn from_group(
        slot: &c::CallbackSlot,
        group: &c::ParameterGroup,
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        match group {
            c::ParameterGroup::Value(index) => Self::from_parameter(slot.parameter(*index)),
            c::ParameterGroup::ByteSlice(bytes) => Self::from_bytes(slot, bytes),
            c::ParameterGroup::DirectVector(vector) => Self::from_direct_vector(slot, vector),
            c::ParameterGroup::CallbackCompletion(completion) => {
                Self::from_completion(slot, completion)
            }
            c::ParameterGroup::Closure(closure) => Self::from_closure(slot, closure, closures),
            c::ParameterGroup::Continuation(_) => Err(Error::UnsupportedBridge {
                bridge: JNI_BRIDGE,
                shape: "callback continuation parameter",
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

    fn from_direct_vector(
        slot: &c::CallbackSlot,
        vector: &c::DirectVectorParameter,
    ) -> Result<Self> {
        Ok(Self {
            kind: CallbackArgumentKind::DirectVector {
                array: Identifier::escape(vector.name())?,
                pointer: CallbackCParameter::from_parameter(slot.parameter(vector.pointer()))?,
                length: CallbackCParameter::from_parameter(slot.parameter(vector.length()))?,
                jni_type: match vector.element() {
                    c::DirectVectorElementAbi::Typed(element) => JniType::from_c_type(element)?,
                    c::DirectVectorElementAbi::PackedBytes => {
                        JniType::from_c_type(&c::Type::Uint8)?
                    }
                },
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

    fn from_closure(
        slot: &c::CallbackSlot,
        closure: &c::ClosureParameter,
        registrations: &[ClosureRegistration],
    ) -> Result<Self> {
        let registration = registrations
            .iter()
            .find(|registration| registration.signature() == closure.signature())
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback closure parameter has no JNI closure registration",
            })?;
        let handle = registration
            .callback_handle()
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback closure parameter has no JNI closure handle",
            })?;

        Ok(Self {
            kind: CallbackArgumentKind::Closure {
                handle: Identifier::parse(format!("__boltffi_{}_handle", closure.name()))?,
                call: CallbackCParameter::from_parameter(slot.parameter(closure.call()))?,
                context: CallbackCParameter::from_parameter(slot.parameter(closure.context()))?,
                release: CallbackCParameter::from_parameter(slot.parameter(closure.release()))?,
                handle_new: handle.new_function().clone(),
                handle_release: handle.release_function().clone(),
            },
        })
    }
}

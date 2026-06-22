use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::{CallbackCParameter, ClosureRegistration, JniType},
    },
    core::{Error, Result},
};

use super::{CallbackArgument, CallbackArgumentKind};

const JNI_BRIDGE: &str = "jni";

impl CallbackArgument {
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

use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
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
}

impl CallbackArgument {
    /// Returns the C ABI parameters that carry this callback argument.
    pub fn c_parameters(&self) -> Vec<CallbackCParameter> {
        match &self.kind {
            CallbackArgumentKind::Value { parameter, .. } => vec![parameter.clone()],
            CallbackArgumentKind::Bytes {
                pointer, length, ..
            } => vec![pointer.clone(), length.clone()],
            CallbackArgumentKind::Record { parameter, .. } => vec![parameter.clone()],
        }
    }

    /// Returns the JNI method descriptor segment for this argument.
    pub fn jni_signature(&self) -> &'static str {
        match &self.kind {
            CallbackArgumentKind::Value { jni_type, .. } => jni_type.signature(),
            CallbackArgumentKind::Bytes { .. } | CallbackArgumentKind::Record { .. } => "[B",
        }
    }

    /// Returns the expression passed to the static JVM callback method.
    pub fn jni_argument(&self) -> Expression {
        match &self.kind {
            CallbackArgumentKind::Value {
                parameter,
                jni_type,
            } => Expression::cast(
                jni_type.as_type_fragment(),
                Expression::identifier(parameter.name.clone()),
            ),
            CallbackArgumentKind::Bytes { name, .. } => Expression::identifier(name.clone()),
            CallbackArgumentKind::Record { array, .. } => Expression::identifier(array.clone()),
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
            CallbackArgumentKind::Record { .. } => None,
        }
    }

    /// Returns record-array setup data when this argument carries a direct record.
    pub fn record(&self) -> Option<CallbackRecordArgument<'_>> {
        match &self.kind {
            CallbackArgumentKind::Value { .. } | CallbackArgumentKind::Bytes { .. } => None,
            CallbackArgumentKind::Record { array, parameter } => Some(CallbackRecordArgument {
                array,
                parameter: &parameter.name,
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

    fn from_parameter(parameter: &c::Parameter) -> Result<Self> {
        Ok(Self {
            name: Identifier::parse(parameter.name())?,
            ty: TypeFragment::anonymous(parameter.ty())?,
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

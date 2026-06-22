mod c_abi;
mod c_bridge;
mod jvm;
mod jvm_setup;

use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::CallbackCParameter,
};

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
        jni_type: crate::bridge::jni::JniType,
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
        jni_type: crate::bridge::jni::JniType,
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

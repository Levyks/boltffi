//! Template views for callback arguments.
//!
//! Callback argument contracts are typed for bridge correctness. This module
//! turns them into the statements, declarations, and argument lists consumed by
//! the callback Askama templates.

use crate::bridge::{
    c::{ArgumentList, Identifier, Statement},
    jni::{
        CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
        CallbackCompletionArgument, CallbackHandleArgument, CallbackRecordArgument,
    },
};

pub struct CallbackCParameterView {
    pub declaration: Statement,
}

pub struct CallbackBytesArgumentView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
}

pub struct CallbackRecordArgumentView {
    pub array: Identifier,
    pub parameter: Identifier,
}

pub struct CallbackHandleArgumentView {
    pub handle: Identifier,
    pub parameter: Identifier,
}

pub struct CallbackClosureArgumentView {
    pub handle: Identifier,
    pub call: Identifier,
    pub context: Identifier,
    pub release: Identifier,
    pub handle_new: Identifier,
    pub handle_release: Identifier,
}

pub struct CallbackCompletionArgumentView {
    pub callback: Identifier,
    pub failure_arguments: ArgumentList,
}

impl CallbackCParameterView {
    pub fn from_parameter(parameter: &CallbackCParameter) -> Self {
        Self {
            declaration: parameter.declaration().clone(),
        }
    }
}

impl CallbackBytesArgumentView {
    pub fn from_argument(argument: &CallbackBytesArgument) -> Self {
        Self {
            name: argument.name().clone(),
            pointer: argument.pointer().clone(),
            length: argument.length().clone(),
        }
    }
}

impl CallbackRecordArgumentView {
    pub fn from_argument(argument: &CallbackRecordArgument) -> Self {
        Self {
            array: argument.array().clone(),
            parameter: argument.parameter().clone(),
        }
    }
}

impl CallbackHandleArgumentView {
    pub fn from_argument(argument: &CallbackHandleArgument) -> Self {
        Self {
            handle: argument.handle().clone(),
            parameter: argument.parameter().clone(),
        }
    }
}

impl CallbackClosureArgumentView {
    pub fn from_argument(argument: &CallbackClosureArgument) -> Self {
        Self {
            handle: argument.handle().clone(),
            call: argument.call().clone(),
            context: argument.context().clone(),
            release: argument.release().clone(),
            handle_new: argument.handle_new().clone(),
            handle_release: argument.handle_release().clone(),
        }
    }
}

impl CallbackCompletionArgumentView {
    pub fn from_argument(argument: &CallbackCompletionArgument) -> Self {
        Self {
            callback: argument.callback().clone(),
            failure_arguments: argument.failure_arguments().clone(),
        }
    }
}

//! Nested closure arguments from C parameter groups.
//!
//! A closure call can carry another closure through call, context, and release
//! parameters. This module finds the matching registered signature and builds
//! the nested JVM closure handle.

use crate::{
    bridge::{
        c,
        jni::{CallbackClosureHandle, ClosureCParameter, ClosureHandleArgument, JvmClassPath},
    },
    core::Result,
};

use super::super::super::names::ClosureNames;
use super::ClosureCall;

pub fn from_group(
    class: &JvmClassPath,
    call: ClosureCall<'_>,
    nested: &c::ClosureParameter,
) -> Result<ClosureHandleArgument> {
    let names = ClosureNames::new(nested.signature());
    let handle = CallbackClosureHandle::new(
        class,
        nested.signature(),
        call.parameter(nested.call()).ty(),
    )?;
    ClosureHandleArgument::new(
        nested.name(),
        ClosureCParameter::from_parameter(call.parameter(nested.call()))?,
        ClosureCParameter::from_parameter(call.parameter(nested.context()))?,
        ClosureCParameter::from_parameter(call.parameter(nested.release()))?,
        &handle,
        names.call()?,
        names.release()?,
    )
}

//! Borrowed-byte closure arguments from C parameter groups.
//!
//! The C bridge groups encoded closure arguments as pointer and length. This
//! module turns that group into the byte-array argument used by the JNI closure
//! trampoline.

use crate::{
    bridge::{c, jni::ClosureBytesArgument},
    core::Result,
};

use super::ClosureCall;

pub fn from_group(
    call: ClosureCall<'_>,
    bytes: &c::ByteSliceParameter,
) -> Result<ClosureBytesArgument> {
    ClosureBytesArgument::from_bytes(
        call.parameter(bytes.pointer()),
        call.parameter(bytes.length()),
        bytes,
    )
}

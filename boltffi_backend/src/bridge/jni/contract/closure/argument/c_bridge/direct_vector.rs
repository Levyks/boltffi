//! Direct-vector closure arguments from C parameter groups.
//!
//! The C bridge groups direct vectors as pointer and length. This module keeps
//! that group as one Java primitive array argument for the closure trampoline.

use crate::{
    bridge::{c, jni::ClosureDirectVectorArgument},
    core::Result,
};

use super::ClosureCall;

pub fn from_group(
    call: ClosureCall<'_>,
    vector: &c::DirectVectorParameter,
) -> Result<ClosureDirectVectorArgument> {
    ClosureDirectVectorArgument::from_vector(
        call.parameter(vector.pointer()),
        call.parameter(vector.length()),
        vector,
    )
}

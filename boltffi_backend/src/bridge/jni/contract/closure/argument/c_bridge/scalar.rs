//! Scalar closure arguments from C parameter groups.
//!
//! Plain closure parameters are single C values. This module maps that value to
//! the JNI scalar argument used by the generated closure trampoline.

use crate::{bridge::c, core::Result};

use super::super::ClosureScalarArgument;
use super::ClosureCall;

pub fn from_index(
    call: ClosureCall<'_>,
    index: c::ParameterIndex,
) -> Result<ClosureScalarArgument> {
    ClosureScalarArgument::from_parameter(call.parameter(index))
}

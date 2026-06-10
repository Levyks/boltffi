use boltffi_binding::{ImportSymbol, Native, NativeSymbol, Surface, Wasm32, native, wasm32};

use crate::experimental::error::Error;

/// A render lane for a foreign-provided closure registering with Rust.
pub enum IncomingClosureLane<'registration> {
    /// Invoke function pointer, context pointer, and release function.
    InvokeContextRelease,
    /// Handle backed by imported invoke and release functions.
    HandleImports {
        /// Import Rust calls to invoke the closure.
        call: &'registration ImportSymbol,
        /// Import Rust calls when the closure handle is released.
        free: &'registration ImportSymbol,
    },
}

/// A render lane for a Rust-provided closure registering with foreign code.
pub enum OutgoingClosureLane<'registration> {
    /// Invoke function pointer, context pointer, and release function.
    InvokeContextRelease,
    /// Handle backed by exported invoke and release functions.
    HandleExports {
        /// Export foreign code calls to invoke the closure.
        call: &'registration NativeSymbol,
        /// Export foreign code calls when releasing the closure handle.
        free: &'registration NativeSymbol,
    },
}

/// How inline closures cross on a surface.
///
/// A closure crosses by registering an invocation surface with the other
/// side. The IR records one registration value per direction; this trait
/// resolves each value to the render lane the wrapper emits.
pub trait ClosureCrossings: Surface {
    /// Resolves a foreign-provided closure registration to its render lane.
    fn incoming_closure_lane<'registration>(
        registration: &'registration Self::IncomingClosureRegistration,
    ) -> Result<IncomingClosureLane<'registration>, Error>;

    /// Resolves a Rust-provided closure registration to its render lane.
    fn outgoing_closure_lane<'registration>(
        registration: &'registration Self::OutgoingClosureRegistration,
    ) -> Result<OutgoingClosureLane<'registration>, Error>;
}

impl ClosureCrossings for Native {
    fn incoming_closure_lane<'registration>(
        registration: &'registration native::ClosureRegistration,
    ) -> Result<IncomingClosureLane<'registration>, Error> {
        match registration {
            native::ClosureRegistration::InvokeContextRelease => {
                Ok(IncomingClosureLane::InvokeContextRelease)
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure registration",
            )),
        }
    }

    fn outgoing_closure_lane<'registration>(
        registration: &'registration native::ClosureRegistration,
    ) -> Result<OutgoingClosureLane<'registration>, Error> {
        match registration {
            native::ClosureRegistration::InvokeContextRelease => {
                Ok(OutgoingClosureLane::InvokeContextRelease)
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure return registration",
            )),
        }
    }
}

impl ClosureCrossings for Wasm32 {
    fn incoming_closure_lane<'registration>(
        registration: &'registration wasm32::IncomingClosureRegistration,
    ) -> Result<IncomingClosureLane<'registration>, Error> {
        Ok(IncomingClosureLane::HandleImports {
            call: registration.call(),
            free: registration.free(),
        })
    }

    fn outgoing_closure_lane<'registration>(
        registration: &'registration wasm32::OutgoingClosureRegistration,
    ) -> Result<OutgoingClosureLane<'registration>, Error> {
        Ok(OutgoingClosureLane::HandleExports {
            call: registration.call(),
            free: registration.free(),
        })
    }
}

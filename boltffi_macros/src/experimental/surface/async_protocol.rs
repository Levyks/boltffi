use boltffi_binding::{Native, NativeSymbol, Surface, Wasm32, native, wasm32};

use crate::experimental::error::Error;

/// How the foreign side drives an async poll loop.
#[derive(Clone, Copy)]
pub enum PollStyle {
    /// Poll registers a continuation callback invoked on readiness.
    Continuation,
    /// Poll advances synchronously and reports readiness in the return value.
    Synchronous,
}

/// Lifecycle symbols of a poll-handle async protocol.
pub struct PollHandleSymbols<'symbols> {
    poll: &'symbols NativeSymbol,
    complete: &'symbols NativeSymbol,
    cancel: &'symbols NativeSymbol,
    free: &'symbols NativeSymbol,
    panic_message: &'symbols NativeSymbol,
    style: PollStyle,
}

impl<'symbols> PollHandleSymbols<'symbols> {
    /// Returns the symbol that advances the operation.
    pub fn poll(&self) -> &'symbols NativeSymbol {
        self.poll
    }

    /// Returns the symbol that extracts the resolved value once ready.
    pub fn complete(&self) -> &'symbols NativeSymbol {
        self.complete
    }

    /// Returns the symbol that requests cancellation.
    pub fn cancel(&self) -> &'symbols NativeSymbol {
        self.cancel
    }

    /// Returns the symbol that releases the async state.
    pub fn free(&self) -> &'symbols NativeSymbol {
        self.free
    }

    /// Returns the symbol that retrieves the panic message after a failed
    /// operation.
    pub fn panic_message(&self) -> &'symbols NativeSymbol {
        self.panic_message
    }

    /// Returns how the foreign side drives the poll loop.
    pub fn style(&self) -> PollStyle {
        self.style
    }
}

/// How async callables run their lifecycle on a surface.
pub trait AsyncLifecycle: Surface {
    /// Resolves a protocol to its poll-handle lifecycle symbols.
    fn poll_handle(protocol: &Self::AsyncProtocol) -> Result<PollHandleSymbols<'_>, Error>;
}

impl AsyncLifecycle for Native {
    fn poll_handle(protocol: &native::AsyncProtocol) -> Result<PollHandleSymbols<'_>, Error> {
        match protocol {
            native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            } => Ok(PollHandleSymbols {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                style: PollStyle::Continuation,
            }),
            _ => Err(Error::UnsupportedExpansion("native async protocol")),
        }
    }
}

impl AsyncLifecycle for Wasm32 {
    fn poll_handle(protocol: &wasm32::AsyncProtocol) -> Result<PollHandleSymbols<'_>, Error> {
        match protocol {
            wasm32::AsyncProtocol::PollHandle {
                poll_sync,
                complete,
                cancel,
                free,
                panic_message,
                ..
            } => Ok(PollHandleSymbols {
                poll: poll_sync,
                complete,
                cancel,
                free,
                panic_message,
                style: PollStyle::Synchronous,
            }),
            _ => Err(Error::UnsupportedExpansion("wasm async protocol")),
        }
    }
}

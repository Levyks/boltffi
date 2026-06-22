use crate::bridge::c::Identifier;

/// Rust-owned closure argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackClosureArgument<'argument> {
    handle: &'argument Identifier,
    call: &'argument Identifier,
    context: &'argument Identifier,
    release: &'argument Identifier,
    handle_new: &'argument Identifier,
    handle_release: &'argument Identifier,
}

impl<'argument> CallbackClosureArgument<'argument> {
    pub(in crate::bridge::jni::contract::callback) fn new(
        handle: &'argument Identifier,
        call: &'argument Identifier,
        context: &'argument Identifier,
        release: &'argument Identifier,
        handle_new: &'argument Identifier,
        handle_release: &'argument Identifier,
    ) -> Self {
        Self {
            handle,
            call,
            context,
            release,
            handle_new,
            handle_release,
        }
    }

    /// Returns the local JVM closure-handle token.
    pub fn handle(&self) -> &Identifier {
        self.handle
    }

    /// Returns the C closure call function parameter.
    pub fn call(&self) -> &Identifier {
        self.call
    }

    /// Returns the C closure context parameter.
    pub fn context(&self) -> &Identifier {
        self.context
    }

    /// Returns the C closure release function parameter.
    pub fn release(&self) -> &Identifier {
        self.release
    }

    /// Returns the helper that stores this closure behind a JNI handle.
    pub fn handle_new(&self) -> &Identifier {
        self.handle_new
    }

    /// Returns the helper that releases this closure handle.
    pub fn handle_release(&self) -> &Identifier {
        self.handle_release
    }
}

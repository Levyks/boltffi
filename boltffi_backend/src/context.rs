use boltffi_binding::{Bindings, Surface};

/// Read-only state shared while one target renders a binding contract.
#[non_exhaustive]
pub struct RenderContext<'bindings, S: Surface> {
    bindings: &'bindings Bindings<S>,
    target: &'static str,
}

impl<'bindings, S: Surface> RenderContext<'bindings, S> {
    /// Creates a render context for a target.
    pub const fn new(bindings: &'bindings Bindings<S>, target: &'static str) -> Self {
        Self { bindings, target }
    }

    /// Returns the binding contract being rendered.
    pub const fn bindings(&self) -> &'bindings Bindings<S> {
        self.bindings
    }

    /// Returns the backend target name.
    pub const fn target(&self) -> &'static str {
        self.target
    }
}

use proc_macro2::Span;
use quote::format_ident;
use syn::Ident;

pub struct Wrapper {
    span: Span,
}

impl Wrapper {
    pub const fn new(span: Span) -> Self {
        Self { span }
    }

    pub fn result(&self) -> Ident {
        Ident::new("__boltffi_result", self.span)
    }

    pub fn error(&self) -> Ident {
        Ident::new("__boltffi_error", self.span)
    }

    pub fn success(&self) -> Ident {
        Ident::new("__boltffi_success", self.span)
    }

    pub fn value(&self) -> Ident {
        Ident::new("__boltffi_value", self.span)
    }

    pub fn return_out(&self) -> Ident {
        Ident::new("__boltffi_return_out", self.span)
    }

    pub fn closure(&self) -> Ident {
        Ident::new("__boltffi_closure", self.span)
    }

    pub fn closure_context(&self) -> Ident {
        Ident::new("__boltffi_closure_context", self.span)
    }

    pub fn receiver(&self) -> Ident {
        Ident::new("__boltffi_receiver", self.span)
    }

    pub fn success_out(&self) -> Ident {
        Ident::new("__boltffi_success_out", self.span)
    }
}

pub struct ClosureRegistration<'source> {
    source: &'source Ident,
}

impl<'source> ClosureRegistration<'source> {
    pub const fn new(source: &'source Ident) -> Self {
        Self { source }
    }

    pub fn call(&self) -> Ident {
        self.ident("call")
    }

    pub fn context(&self) -> Ident {
        self.ident("context")
    }

    pub fn release(&self) -> Ident {
        self.ident("release")
    }

    pub fn owner(&self) -> Ident {
        self.ident("owner")
    }

    fn ident(&self, role: &str) -> Ident {
        let text = self.source.to_string();
        let stem = text.strip_prefix("__boltffi_").unwrap_or(&text);
        Ident::new(&format!("__boltffi_{stem}_{role}"), self.source.span())
    }
}

pub struct NativeClosureRegistration<'source> {
    source: &'source Ident,
}

impl<'source> NativeClosureRegistration<'source> {
    pub const fn new(source: &'source Ident) -> Self {
        Self { source }
    }

    pub fn call(&self) -> Ident {
        self.ident("call")
    }

    pub fn context(&self) -> Ident {
        self.ident("context")
    }

    pub fn release(&self) -> Ident {
        self.ident("release")
    }

    fn ident(&self, role: &str) -> Ident {
        format_ident!(
            "__boltffi_{}_{}",
            self.source,
            role,
            span = self.source.span()
        )
    }
}

pub struct ReturnedClosureRegistration<'source> {
    owner: &'source Ident,
    channel: &'source str,
}

impl<'source> ReturnedClosureRegistration<'source> {
    pub const fn new(owner: &'source Ident, channel: &'source str) -> Self {
        Self { owner, channel }
    }

    pub fn call(&self) -> Ident {
        self.ident("call")
    }

    pub fn release(&self) -> Ident {
        self.ident("release")
    }

    fn ident(&self, role: &str) -> Ident {
        format_ident!(
            "__boltffi_{}_{}_{}",
            self.owner,
            self.channel,
            role,
            span = self.owner.span()
        )
    }
}

pub struct Parameter<'source> {
    source: &'source Ident,
}

impl<'source> Parameter<'source> {
    pub const fn new(source: &'source Ident) -> Self {
        Self { source }
    }

    pub fn pointer(&self) -> Ident {
        self.ident("ptr")
    }

    pub fn length(&self) -> Ident {
        self.ident("len")
    }

    pub fn storage(&self) -> Ident {
        self.ident("storage")
    }

    pub fn buffer(&self) -> Ident {
        self.ident("buffer")
    }

    pub fn writeback(&self) -> Ident {
        self.ident("out")
    }

    pub fn packed(&self) -> Ident {
        self.ident("packed")
    }

    pub fn handle(&self) -> Ident {
        self.ident("handle")
    }

    fn ident(&self, role: &str) -> Ident {
        let text = self.source.to_string();
        let stem = text.strip_prefix("__boltffi_").unwrap_or(&text);
        Ident::new(&format!("__boltffi_{stem}_{role}"), self.source.span())
    }
}

pub struct ClosureArgument {
    index: usize,
}

impl ClosureArgument {
    pub const fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn value(&self) -> Ident {
        format_ident!("__boltffi_arg{}", self.index)
    }

    pub fn ffi(&self) -> Ident {
        format_ident!("__boltffi_ffi_arg{}", self.index)
    }

    pub fn pointer(&self) -> Ident {
        format_ident!("__boltffi_arg{}_ptr", self.index)
    }

    pub fn length(&self) -> Ident {
        format_ident!("__boltffi_arg{}_len", self.index)
    }

    pub fn wire(&self) -> Ident {
        format_ident!("__boltffi_arg{}_wire", self.index)
    }
}

pub struct RecordField<'source> {
    source: &'source Ident,
}

impl<'source> RecordField<'source> {
    pub const fn new(source: &'source Ident) -> Self {
        Self { source }
    }

    pub fn decoded(&self) -> Ident {
        self.ident("decoded")
    }

    pub fn used(&self) -> Ident {
        self.ident("used")
    }

    pub fn wire(&self) -> Ident {
        self.ident("wire")
    }

    fn ident(&self, role: &str) -> Ident {
        format_ident!(
            "__boltffi_{}_{}",
            self.source,
            role,
            span = self.source.span()
        )
    }
}

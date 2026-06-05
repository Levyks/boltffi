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

    pub fn success_out(&self) -> Ident {
        Ident::new("__boltffi_success_out", self.span)
    }
}

pub struct Parameter<'a> {
    source: &'a Ident,
}

impl<'a> Parameter<'a> {
    pub const fn new(source: &'a Ident) -> Self {
        Self { source }
    }

    pub fn pointer(&self) -> Ident {
        format_ident!("__boltffi_{}_ptr", self.source, span = self.source.span())
    }

    pub fn length(&self) -> Ident {
        format_ident!("__boltffi_{}_len", self.source, span = self.source.span())
    }

    pub fn storage(&self) -> Ident {
        format_ident!(
            "__boltffi_{}_storage",
            self.source,
            span = self.source.span()
        )
    }

    pub fn writeback(&self) -> Ident {
        format_ident!("__boltffi_{}_out", self.source, span = self.source.span())
    }

    pub fn handle(&self) -> Ident {
        format_ident!(
            "__boltffi_{}_handle",
            self.source,
            span = self.source.span()
        )
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

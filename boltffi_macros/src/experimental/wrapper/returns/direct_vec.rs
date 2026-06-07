use boltffi_binding::{Native, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{error::Error, wrapper::Render};

use super::Tokens;

pub struct Renderer;
pub struct Failure;
pub struct FailureInput;
pub struct Empty;

pub struct Input {
    value: syn::Ident,
}

impl Input {
    pub fn new(value: syn::Ident) -> Self {
        Self { value }
    }
}

impl Render<Native, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let value = input.value;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> ::boltffi::__private::FfiBuf },
            body: quote! {
                <_ as ::boltffi::__private::VecTransport>::pack_vec(#value)
            },
        })
    }
}

impl Render<Wasm32, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let value = input.value;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! {},
            body: quote! {
                let __boltffi_buf = ::boltffi::__private::FfiBuf::from_vec(#value);
                ::boltffi::__private::write_return_slot(
                    __boltffi_buf.as_ptr() as u32,
                    __boltffi_buf.len() as u32,
                    __boltffi_buf.cap() as u32,
                    __boltffi_buf.align() as u32
                );
                core::mem::forget(__boltffi_buf);
            },
        })
    }
}

impl Render<Native, FailureInput> for Failure {
    type Output = TokenStream;

    fn render(self, _input: FailureInput) -> Result<Self::Output, Error> {
        let empty = <Renderer as Render<Native, Empty>>::render(Renderer, Empty)?;
        let body = empty.body();
        Ok(quote! {
            return #body;
        })
    }
}

impl Render<Wasm32, FailureInput> for Failure {
    type Output = TokenStream;

    fn render(self, _input: FailureInput) -> Result<Self::Output, Error> {
        let empty = <Renderer as Render<Wasm32, Empty>>::render(Renderer, Empty)?;
        let body = empty.body();
        Ok(quote! {
            return #body;
        })
    }
}

impl Render<Native, Empty> for Renderer {
    type Output = Tokens;

    fn render(self, _input: Empty) -> Result<Self::Output, Error> {
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> ::boltffi::__private::FfiBuf },
            body: quote! { ::boltffi::__private::FfiBuf::default() },
        })
    }
}

impl Render<Wasm32, Empty> for Renderer {
    type Output = Tokens;

    fn render(self, _input: Empty) -> Result<Self::Output, Error> {
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! {},
            body: TokenStream::new(),
        })
    }
}

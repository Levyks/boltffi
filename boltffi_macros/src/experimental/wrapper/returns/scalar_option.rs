use boltffi_binding::{Native, Primitive, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    wrapper::{Render, names, returns::Tokens},
};

pub struct Renderer;
pub struct Failure;
pub struct FailureInput;
pub struct Empty;

pub struct Input {
    primitive: Primitive,
    value: syn::Ident,
}

impl Input {
    pub fn new(primitive: Primitive, value: syn::Ident) -> Self {
        Self { primitive, value }
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
                ::boltffi::__private::FfiBuf::wire_encode(&#value)
            },
        })
    }
}

impl Render<Wasm32, Input> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        let value = input.value;
        let present = names::Wrapper::new(value.span()).value();
        let some = Scalar::new(input.primitive, &present).tokens()?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> f64 },
            body: quote! {
                match #value {
                    Some(#present) => #some,
                    None => f64::NAN,
                }
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
            return_type: quote! { -> f64 },
            body: quote! { f64::NAN },
        })
    }
}

pub struct Scalar<'value> {
    primitive: Primitive,
    value: &'value syn::Ident,
}

impl<'value> Scalar<'value> {
    pub fn new(primitive: Primitive, value: &'value syn::Ident) -> Self {
        Self { primitive, value }
    }

    pub fn tokens(self) -> Result<TokenStream, Error> {
        let value = self.value;
        Ok(match self.primitive {
            Primitive::Bool => quote! {
                if #value { 1.0 } else { 0.0 }
            },
            Primitive::F64 => quote! { #value },
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32 => quote! {
                #value as f64
            },
            _ => return Err(Error::UnsupportedExpansion("scalar option primitive")),
        })
    }
}

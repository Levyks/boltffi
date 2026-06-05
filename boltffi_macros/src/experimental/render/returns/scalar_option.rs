use boltffi_binding::{Native, Primitive, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    render::{Rule as RenderRule, local, returns::Tokens},
};

pub struct Rule;
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

impl RenderRule<Native, Input> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input) -> Result<Self::Output, Error> {
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

impl RenderRule<Wasm32, Input> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input) -> Result<Self::Output, Error> {
        let value = input.value;
        let present = local::Wrapper::new(value.span()).value();
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

impl RenderRule<Native, FailureInput> for Failure {
    type Output = TokenStream;

    fn apply(self, _input: FailureInput) -> Result<Self::Output, Error> {
        let empty = <Rule as RenderRule<Native, Empty>>::apply(Rule, Empty)?;
        let body = empty.body();
        Ok(quote! {
            return #body;
        })
    }
}

impl RenderRule<Wasm32, FailureInput> for Failure {
    type Output = TokenStream;

    fn apply(self, _input: FailureInput) -> Result<Self::Output, Error> {
        let empty = <Rule as RenderRule<Wasm32, Empty>>::apply(Rule, Empty)?;
        let body = empty.body();
        Ok(quote! {
            return #body;
        })
    }
}

impl RenderRule<Native, Empty> for Rule {
    type Output = Tokens;

    fn apply(self, _input: Empty) -> Result<Self::Output, Error> {
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> ::boltffi::__private::FfiBuf },
            body: quote! { ::boltffi::__private::FfiBuf::default() },
        })
    }
}

impl RenderRule<Wasm32, Empty> for Rule {
    type Output = Tokens;

    fn apply(self, _input: Empty) -> Result<Self::Output, Error> {
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: Vec::new(),
            return_type: quote! { -> f64 },
            body: quote! { f64::NAN },
        })
    }
}

pub struct Scalar<'a> {
    primitive: Primitive,
    value: &'a syn::Ident,
}

impl<'a> Scalar<'a> {
    pub fn new(primitive: Primitive, value: &'a syn::Ident) -> Self {
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

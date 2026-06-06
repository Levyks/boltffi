use boltffi_binding::{Native, Receive, Wasm32, WritePlan, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::PatType;

use crate::experimental::{
    error::Error,
    render::{Rule as RenderRule, codec, local},
    target::Target,
};

use super::Tokens;

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    codec: &'binding WritePlan,
    shape: S::BufferShape,
    receive: Receive,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: TokenStream,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        codec: &'binding WritePlan,
        shape: S::BufferShape,
        receive: Receive,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            codec,
            shape,
            receive,
            syntax,
            ident,
            failure,
        }
    }
}

impl<'binding, 'syntax> RenderRule<Native, Input<'binding, 'syntax, Native>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, Native>) -> Result<Self::Output, Error> {
        match input.shape {
            native::BufferShape::Slice => Slice::new(input).tokens(),
            native::BufferShape::Buffer | native::BufferShape::BufferPointer => Err(
                Error::UnsupportedExpansion("native encoded parameter shape"),
            ),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native encoded parameter shape",
            )),
        }
    }
}

impl<'binding, 'syntax> RenderRule<Wasm32, Input<'binding, 'syntax, Wasm32>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, Wasm32>) -> Result<Self::Output, Error> {
        match input.shape {
            wasm32::BufferShape::Slice => Slice::new(input).tokens(),
            wasm32::BufferShape::Packed => {
                Err(Error::UnsupportedExpansion("wasm encoded parameter shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm encoded parameter shape",
            )),
        }
    }
}

struct Slice<'binding, 'syntax> {
    codec: &'binding WritePlan,
    receive: Receive,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    pointer: syn::Ident,
    length: syn::Ident,
    failure: TokenStream,
}

impl<'binding, 'syntax, S: Target> From<Input<'binding, 'syntax, S>> for Slice<'binding, 'syntax> {
    fn from(input: Input<'binding, 'syntax, S>) -> Self {
        Self::new(input)
    }
}

impl<'binding, 'syntax> Slice<'binding, 'syntax> {
    fn new<S: Target>(input: Input<'binding, 'syntax, S>) -> Self {
        let ident = input.ident;
        let locals = local::Parameter::new(ident);
        Self {
            codec: input.codec,
            receive: input.receive,
            syntax: input.syntax,
            ident,
            pointer: locals.pointer(),
            length: locals.length(),
            failure: input.failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let ident = self.ident;
        let pointer_type = self.pointer_type();
        let conversion =
            codec::EncodedValue::new(self.codec.root()).conversion(codec::DecodeInput::new(
                self.receive,
                self.syntax.ty.as_ref(),
                ident,
                pointer,
                length,
                &self.failure,
            ))?;

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![
                quote! { #pointer: #pointer_type },
                quote! { #length: usize },
            ],
            ffi_parameter_types: vec![pointer_type, quote! { usize }],
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn pointer_type(&self) -> TokenStream {
        quote! { *const u8 }
    }
}

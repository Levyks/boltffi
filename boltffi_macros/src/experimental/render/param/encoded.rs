use boltffi_binding::{Native, Receive, Wasm32, WritePlan, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    render::{Rule as RenderRule, codec, local},
    target::Target,
};

use super::Tokens;

pub struct Rule;

pub struct Input<'binding, S: Target> {
    codec: &'binding WritePlan,
    shape: S::BufferShape,
    receive: Receive,
    rust_type: Type,
    ident: Ident,
    failure: TokenStream,
}

impl<'binding, S: Target> Input<'binding, S> {
    pub fn new(
        codec: &'binding WritePlan,
        shape: S::BufferShape,
        receive: Receive,
        rust_type: Type,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            codec,
            shape,
            receive,
            rust_type,
            ident,
            failure,
        }
    }
}

impl<'binding> RenderRule<Native, Input<'binding, Native>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, Native>) -> Result<Self::Output, Error> {
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

impl<'binding> RenderRule<Wasm32, Input<'binding, Wasm32>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, Wasm32>) -> Result<Self::Output, Error> {
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

struct Slice<'binding> {
    codec: &'binding WritePlan,
    receive: Receive,
    rust_type: Type,
    ident: Ident,
    pointer: Ident,
    length: Ident,
    failure: TokenStream,
}

impl<'binding, S: Target> From<Input<'binding, S>> for Slice<'binding> {
    fn from(input: Input<'binding, S>) -> Self {
        Self::new(input)
    }
}

impl<'binding> Slice<'binding> {
    fn new<S: Target>(input: Input<'binding, S>) -> Self {
        let ident = input.ident;
        let locals = local::Parameter::new(&ident);
        let pointer = locals.pointer();
        let length = locals.length();
        Self {
            codec: input.codec,
            receive: input.receive,
            rust_type: input.rust_type,
            ident,
            pointer,
            length,
            failure: input.failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let ident = &self.ident;
        let pointer_type = self.pointer_type();
        let conversion =
            codec::EncodedValue::new(self.codec.root()).conversion(codec::DecodeInput::new(
                self.receive,
                &self.rust_type,
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

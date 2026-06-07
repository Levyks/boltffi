use boltffi_binding::{Native, Wasm32, WritePlan, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{Render, encoded, names},
};

use super::Tokens;

pub struct Renderer;

pub struct Input<'context, 'binding, S: Target> {
    codec: &'binding WritePlan,
    shape: S::BufferShape,
    target: rust_api::DecodeTarget<'binding>,
    ident: Ident,
    failure: TokenStream,
    expansion: &'context Expansion<'binding, S>,
}

impl<'context, 'binding, S: Target> Input<'context, 'binding, S> {
    pub fn new(
        codec: &'binding WritePlan,
        shape: S::BufferShape,
        target: rust_api::DecodeTarget<'binding>,
        ident: Ident,
        failure: TokenStream,
        expansion: &'context Expansion<'binding, S>,
    ) -> Self {
        Self {
            codec,
            shape,
            target,
            ident,
            failure,
            expansion,
        }
    }
}

impl<'context, 'binding> Render<Native, Input<'context, 'binding, Native>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'context, 'binding, Native>) -> Result<Self::Output, Error> {
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

impl<'context, 'binding> Render<Wasm32, Input<'context, 'binding, Wasm32>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'context, 'binding, Wasm32>) -> Result<Self::Output, Error> {
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

struct Slice<'context, 'binding, S: Target> {
    codec: &'binding WritePlan,
    target: rust_api::DecodeTarget<'binding>,
    ident: Ident,
    pointer: Ident,
    length: Ident,
    failure: TokenStream,
    expansion: &'context Expansion<'binding, S>,
}

impl<'context, 'binding, S: Target> From<Input<'context, 'binding, S>>
    for Slice<'context, 'binding, S>
{
    fn from(input: Input<'context, 'binding, S>) -> Self {
        Self::new(input)
    }
}

impl<'context, 'binding, S: Target> Slice<'context, 'binding, S> {
    fn new(input: Input<'context, 'binding, S>) -> Self {
        let ident = input.ident;
        let locals = names::Parameter::new(&ident);
        let pointer = locals.pointer();
        let length = locals.length();
        Self {
            codec: input.codec,
            target: input.target,
            ident,
            pointer,
            length,
            failure: input.failure,
            expansion: input.expansion,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let ident = &self.ident;
        let pointer_type = self.pointer_type();
        let conversion = encoded::incoming::Value::new(self.codec.root(), self.expansion).decode(
            encoded::incoming::Input::new(&self.target, ident, pointer, length, &self.failure),
        )?;

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

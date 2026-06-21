use boltffi_binding::{Native, Wasm32, WritePlan, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{Render, encoded, names},
};

use super::Tokens;

pub struct Renderer;

pub struct Input<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered WritePlan,
    shape: S::BufferShape,
    target: rust_api::DecodeTarget,
    ident: Ident,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
    writeback: bool,
}

impl<'expansion, 'lowered, S: RenderSurface> Input<'expansion, 'lowered, S> {
    pub fn new(
        codec: &'lowered WritePlan,
        shape: S::BufferShape,
        target: rust_api::DecodeTarget,
        ident: Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            codec,
            shape,
            target,
            ident,
            failure,
            expansion,
            writeback: false,
        }
    }

    pub fn with_writeback(mut self) -> Self {
        self.writeback = true;
        self
    }
}

impl<'expansion, 'lowered> Render<Native, Input<'expansion, 'lowered, Native>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
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

impl<'expansion, 'lowered> Render<Wasm32, Input<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        match input.shape {
            wasm32::BufferShape::Slice => {
                let mut input = input;
                input.writeback = false;
                Slice::new(input).tokens()
            }
            wasm32::BufferShape::Packed => {
                Err(Error::UnsupportedExpansion("wasm encoded parameter shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm encoded parameter shape",
            )),
        }
    }
}

struct Slice<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered WritePlan,
    target: rust_api::DecodeTarget,
    ident: Ident,
    pointer: Ident,
    length: Ident,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
    writeback: bool,
}

impl<'expansion, 'lowered, S: RenderSurface> From<Input<'expansion, 'lowered, S>>
    for Slice<'expansion, 'lowered, S>
{
    fn from(input: Input<'expansion, 'lowered, S>) -> Self {
        Self::new(input)
    }
}

impl<'expansion, 'lowered, S: RenderSurface> Slice<'expansion, 'lowered, S> {
    fn new(input: Input<'expansion, 'lowered, S>) -> Self {
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
            writeback: input.writeback,
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
        let writeback = self.writeback()?;

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: [
                quote! { #pointer: #pointer_type },
                quote! { #length: usize },
            ]
            .into_iter()
            .chain(writeback.ffi_parameters)
            .collect(),
            ffi_parameter_types: [pointer_type, quote! { usize }]
                .into_iter()
                .chain(writeback.ffi_parameter_types)
                .collect(),
            conversions: std::iter::once(conversion)
                .chain(writeback.conversions)
                .collect(),
            writebacks: writeback.writebacks,
            argument: quote! { #ident },
        })
    }

    fn pointer_type(&self) -> TokenStream {
        quote! { *const u8 }
    }

    fn writeback(&self) -> Result<Writeback, Error> {
        if !self.writeback {
            return Ok(Writeback::none());
        }
        let out = names::Parameter::new(&self.ident).writeback();
        let storage = names::Parameter::new(&self.ident).storage();
        let failure = &self.failure;
        // The native C bridge cannot mutate the inbound byte slice in place, so mutable encoded params write changed storage into a separate owned buffer for the host wrapper to decode.
        let buffer =
            encoded::outgoing::Value::new(self.codec.root(), self.expansion).buffer(quote! {
                #storage
            })?;
        Ok(Writeback {
            ffi_parameters: vec![quote! { #out: *mut ::boltffi::__private::FfiBuf }],
            ffi_parameter_types: vec![quote! { *mut ::boltffi::__private::FfiBuf }],
            conversions: vec![quote! {
                if #out.is_null() {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: writeback pointer is null",
                        stringify!(#out)
                    ));
                    #failure
                }
            }],
            writebacks: vec![quote! {
                unsafe {
                    ::core::ptr::write(#out, #buffer);
                }
            }],
        })
    }
}

struct Writeback {
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
}

impl Writeback {
    fn none() -> Self {
        Self {
            ffi_parameters: Vec::new(),
            ffi_parameter_types: Vec::new(),
            conversions: Vec::new(),
            writebacks: Vec::new(),
        }
    }
}

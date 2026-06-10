use boltffi_binding::{Native, ReadPlan, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    target::Target,
    wrapper::{Render, encoded},
};

pub struct Renderer;

pub struct Input<'expansion, 'lowered, S: Target> {
    codec: &'lowered ReadPlan,
    shape: S::BufferShape,
    value: syn::Ident,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: Target> Input<'expansion, 'lowered, S> {
    pub fn new(
        codec: &'lowered ReadPlan,
        shape: S::BufferShape,
        value: syn::Ident,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            codec,
            shape,
            value,
            expansion,
        }
    }

    pub fn string(
        codec: &'lowered ReadPlan,
        shape: S::BufferShape,
        value: syn::Ident,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            codec,
            shape,
            value,
            expansion,
        }
    }
}

pub struct Tokens {
    value_type: TokenStream,
    return_type: TokenStream,
    value: TokenStream,
}

impl Tokens {
    pub fn return_type(&self) -> &TokenStream {
        &self.return_type
    }

    pub fn value(&self) -> &TokenStream {
        &self.value
    }

    pub fn return_type_without_arrow(&self) -> TokenStream {
        self.value_type.clone()
    }
}

pub struct Empty<S: Target> {
    shape: S::BufferShape,
}

impl<S: Target> Empty<S> {
    pub fn new(shape: S::BufferShape) -> Self {
        Self { shape }
    }
}

impl<'expansion, 'lowered> Render<Native, Input<'expansion, 'lowered, Native>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        let value = input.value;
        match input.shape {
            native::BufferShape::Buffer => {
                let value = encoded::outgoing::Value::new(input.codec.root(), input.expansion)
                    .buffer(quote! { #value })?;
                Ok(Tokens {
                    value_type: quote! { ::boltffi::__private::FfiBuf },
                    return_type: quote! { -> ::boltffi::__private::FfiBuf },
                    value,
                })
            }
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnsupportedExpansion("native encoded return shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown native encoded return shape",
            )),
        }
    }
}

impl Render<Native, Empty<Native>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Empty<Native>) -> Result<Self::Output, Error> {
        match input.shape {
            native::BufferShape::Buffer => Ok(Tokens {
                value_type: quote! { ::boltffi::__private::FfiBuf },
                return_type: quote! { -> ::boltffi::__private::FfiBuf },
                value: quote! { ::boltffi::__private::FfiBuf::default() },
            }),
            native::BufferShape::Slice | native::BufferShape::BufferPointer => {
                Err(Error::UnsupportedExpansion("native encoded return shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown native encoded return shape",
            )),
        }
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Input<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        let value = input.value;

        match input.shape {
            wasm32::BufferShape::Packed => {
                let buffer = encoded::outgoing::Value::new(input.codec.root(), input.expansion)
                    .buffer(quote! { #value })?;
                Ok(Tokens {
                    value_type: quote! { u64 },
                    return_type: quote! { -> u64 },
                    value: quote! { #buffer.into_packed() },
                })
            }
            wasm32::BufferShape::Slice => {
                Err(Error::UnsupportedExpansion("wasm encoded return shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm encoded return shape",
            )),
        }
    }
}

impl Render<Wasm32, Empty<Wasm32>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Empty<Wasm32>) -> Result<Self::Output, Error> {
        match input.shape {
            wasm32::BufferShape::Packed => Ok(Tokens {
                value_type: quote! { u64 },
                return_type: quote! { -> u64 },
                value: quote! { ::boltffi::__private::FfiBuf::default().into_packed() },
            }),
            wasm32::BufferShape::Slice => {
                Err(Error::UnsupportedExpansion("wasm encoded return shape"))
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown wasm encoded return shape",
            )),
        }
    }
}

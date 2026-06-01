use boltffi_binding::{Native, TypeRef, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{error::Error, render::Rule as RenderRule, target::Target};

pub struct Rule;

pub struct Input<'a, S: Target> {
    ty: &'a TypeRef,
    shape: S::BufferShape,
    value: syn::Ident,
}

impl<'a, S: Target> Input<'a, S> {
    pub fn new(ty: &'a TypeRef, shape: S::BufferShape, value: syn::Ident) -> Self {
        Self { ty, shape, value }
    }
}

pub struct Tokens {
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
}

impl<'a> RenderRule<Native, Input<'a, Native>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'a, Native>) -> Result<Self::Output, Error> {
        let value = input.value;
        match input.shape {
            native::BufferShape::Buffer => Ok(Tokens {
                return_type: quote! { -> ::boltffi::__private::FfiBuf },
                value: quote! { ::boltffi::__private::FfiBuf::wire_encode(&#value) },
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

impl<'a> RenderRule<Wasm32, Input<'a, Wasm32>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'a, Wasm32>) -> Result<Self::Output, Error> {
        let value = input.value;
        let buffer = match input.ty {
            TypeRef::String => {
                quote! { ::boltffi::__private::FfiBuf::from_vec(#value.into_bytes()) }
            }
            TypeRef::Bytes => quote! { ::boltffi::__private::FfiBuf::from_vec(#value) },
            _ => quote! { ::boltffi::__private::FfiBuf::wire_encode(&#value) },
        };

        match input.shape {
            wasm32::BufferShape::Packed => Ok(Tokens {
                return_type: quote! { -> u64 },
                value: quote! { #buffer.into_packed() },
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

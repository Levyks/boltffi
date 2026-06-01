use boltffi_binding::{Native, Receive, TypeRef, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::experimental::{error::Error, render::Rule as RenderRule, target::Target};

use super::Tokens;

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    ty: &'binding TypeRef,
    shape: S::BufferShape,
    receive: Receive,
    ident: &'syntax syn::Ident,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        ty: &'binding TypeRef,
        shape: S::BufferShape,
        receive: Receive,
        ident: &'syntax syn::Ident,
    ) -> Self {
        Self {
            ty,
            shape,
            receive,
            ident,
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
    ty: &'binding TypeRef,
    receive: Receive,
    ident: &'syntax syn::Ident,
    pointer: syn::Ident,
    length: syn::Ident,
}

impl<'binding, 'syntax, S: Target> From<Input<'binding, 'syntax, S>> for Slice<'binding, 'syntax> {
    fn from(input: Input<'binding, 'syntax, S>) -> Self {
        Self::new(input)
    }
}

impl<'binding, 'syntax> Slice<'binding, 'syntax> {
    fn new<S: Target>(input: Input<'binding, 'syntax, S>) -> Self {
        let ident = input.ident;
        Self {
            ty: input.ty,
            receive: input.receive,
            ident,
            pointer: format_ident!("__boltffi_{}_ptr", ident),
            length: format_ident!("__boltffi_{}_len", ident),
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let ident = self.ident;
        let conversion = match self.ty {
            TypeRef::String => self.string_conversion()?,
            TypeRef::Bytes => self.bytes_conversion()?,
            _ => return Err(Error::UnsupportedExpansion("encoded parameter")),
        };

        Ok(Tokens {
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            conversions: vec![conversion],
            argument: quote! { #ident },
        })
    }

    fn string_conversion(&self) -> Result<TokenStream, Error> {
        let ident = self.ident;
        let pointer = &self.pointer;
        let length = &self.length;
        match self.receive {
            Receive::ByValue => Ok(quote! {
                let #ident: String = if #pointer.is_null() {
                    String::new()
                } else {
                    match ::core::str::from_utf8(unsafe {
                        ::core::slice::from_raw_parts(#pointer, #length)
                    }) {
                        Ok(value) => value.to_string(),
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: invalid UTF-8: {} (buf_len={})",
                                stringify!(#ident),
                                error,
                                #length
                            ));
                            String::new()
                        }
                    }
                };
            }),
            Receive::ByRef => Ok(quote! {
                let #ident: &str = if #pointer.is_null() {
                    ""
                } else {
                    match ::core::str::from_utf8(unsafe {
                        ::core::slice::from_raw_parts(#pointer, #length)
                    }) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: invalid UTF-8: {} (buf_len={})",
                                stringify!(#ident),
                                error,
                                #length
                            ));
                            ""
                        }
                    }
                };
            }),
            Receive::ByMutRef => Err(Error::UnsupportedExpansion(
                "mutable-reference encoded string parameter",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown encoded string parameter receive mode",
            )),
        }
    }

    fn bytes_conversion(&self) -> Result<TokenStream, Error> {
        let ident = self.ident;
        let pointer = &self.pointer;
        let length = &self.length;
        match self.receive {
            Receive::ByValue => Ok(quote! {
                let #ident: Vec<u8> = if #pointer.is_null() {
                    Vec::new()
                } else {
                    unsafe { ::core::slice::from_raw_parts(#pointer, #length) }.to_vec()
                };
            }),
            Receive::ByRef => Ok(quote! {
                let #ident: &[u8] = if #pointer.is_null() {
                    &[]
                } else {
                    unsafe { ::core::slice::from_raw_parts(#pointer, #length) }
                };
            }),
            Receive::ByMutRef => Err(Error::UnsupportedExpansion(
                "mutable-reference encoded bytes parameter",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown encoded bytes parameter receive mode",
            )),
        }
    }
}

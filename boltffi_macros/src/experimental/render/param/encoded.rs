use boltffi_binding::{Native, Receive, TypeRef, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{PatType, Type};

use crate::experimental::{
    error::Error,
    render::{Rule as RenderRule, local},
    target::Target,
};

use super::Tokens;

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    ty: &'binding TypeRef,
    shape: S::BufferShape,
    receive: Receive,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: TokenStream,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        ty: &'binding TypeRef,
        shape: S::BufferShape,
        receive: Receive,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            ty,
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
    ty: &'binding TypeRef,
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
            ty: input.ty,
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
        let conversion = match self.ty {
            TypeRef::String => self.string_conversion()?,
            TypeRef::Bytes => self.bytes_conversion()?,
            _ => self.generic_conversion()?,
        };

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
        match (self.ty, self.receive) {
            (TypeRef::String | TypeRef::Bytes, Receive::ByMutRef) => quote! { *mut u8 },
            _ => quote! { *const u8 },
        }
    }

    fn string_conversion(&self) -> Result<TokenStream, Error> {
        let ident = self.ident;
        let pointer = &self.pointer;
        let length = &self.length;
        let failure = &self.failure;
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
                            #failure
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
                            #failure
                        }
                    }
                };
            }),
            Receive::ByMutRef => {
                let storage = local::Parameter::new(ident).storage();
                Ok(quote! {
                    let mut #storage = String::new();
                    let #ident: &mut str = if #pointer.is_null() {
                        #storage.as_mut_str()
                    } else {
                        match ::core::str::from_utf8_mut(unsafe {
                            ::core::slice::from_raw_parts_mut(#pointer, #length)
                        }) {
                            Ok(value) => value,
                            Err(error) => {
                                ::boltffi::__private::set_last_error(format!(
                                    "{}: invalid UTF-8: {} (buf_len={})",
                                    stringify!(#ident),
                                    error,
                                    #length
                                ));
                                #failure
                            }
                        }
                    };
                })
            }
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
            Receive::ByMutRef => Ok(quote! {
                let #ident: &mut [u8] = if #pointer.is_null() {
                    &mut []
                } else {
                    unsafe { ::core::slice::from_raw_parts_mut(#pointer, #length) }
                };
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown encoded bytes parameter receive mode",
            )),
        }
    }

    fn generic_conversion(&self) -> Result<TokenStream, Error> {
        match self.receive {
            Receive::ByValue => {
                self.generic_value_conversion(self.syntax.ty.as_ref(), self.ident, false)
            }
            Receive::ByRef => {
                let Type::Reference(reference) = self.syntax.ty.as_ref() else {
                    return Err(Error::SourceSyntaxMismatch(
                        "shared-reference encoded parameter syntax does not match binding receive mode",
                    ));
                };
                if reference.mutability.is_some() {
                    return Err(Error::SourceSyntaxMismatch(
                        "shared-reference encoded parameter syntax does not match binding receive mode",
                    ));
                }
                let storage = local::Parameter::new(self.ident).storage();
                let value =
                    self.generic_value_conversion(reference.elem.as_ref(), &storage, false)?;
                let ident = self.ident;
                Ok(quote! {
                    #value
                    let #ident = &#storage;
                })
            }
            Receive::ByMutRef => {
                let Type::Reference(reference) = self.syntax.ty.as_ref() else {
                    return Err(Error::SourceSyntaxMismatch(
                        "mutable-reference encoded parameter syntax does not match binding receive mode",
                    ));
                };
                if reference.mutability.is_none() {
                    return Err(Error::SourceSyntaxMismatch(
                        "mutable-reference encoded parameter syntax does not match binding receive mode",
                    ));
                }
                let storage = local::Parameter::new(self.ident).storage();
                let value =
                    self.generic_value_conversion(reference.elem.as_ref(), &storage, true)?;
                let ident = self.ident;
                Ok(quote! {
                    #value
                    let #ident = &mut #storage;
                })
            }
            _ => Err(Error::UnsupportedExpansion(
                "unknown encoded parameter receive mode",
            )),
        }
    }

    fn generic_value_conversion(
        &self,
        rust_type: &Type,
        binding: &syn::Ident,
        mutable: bool,
    ) -> Result<TokenStream, Error> {
        let pointer = &self.pointer;
        let length = &self.length;
        let failure = &self.failure;
        let mutability = mutable.then(|| quote! { mut });
        if let Some(empty) = self.empty_value() {
            return Ok(quote! {
                let #mutability #binding: #rust_type = if #pointer.is_null() || #length == 0 {
                    #empty
                } else {
                    match ::boltffi::__private::wire::decode::<#rust_type>(unsafe {
                        ::core::slice::from_raw_parts(#pointer, #length)
                    }) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: wire decode failed: {} (buf_len={})",
                                stringify!(#binding),
                                error,
                                #length
                            ));
                            #empty
                        }
                    }
                };
            });
        }

        Ok(quote! {
            let #mutability #binding: #rust_type = {
                if #pointer.is_null() && #length > 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null pointer with non-zero length (buf_len={})",
                        stringify!(#binding),
                        #length
                    ));
                    #failure
                }
                let __boltffi_bytes: &[u8] = if #length == 0 {
                    &[]
                } else {
                    unsafe { ::core::slice::from_raw_parts(#pointer, #length) }
                };
                match ::boltffi::__private::wire::decode(__boltffi_bytes) {
                    Ok(value) => value,
                    Err(error) => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: wire decode failed: {} (buf_len={})",
                            stringify!(#binding),
                            error,
                            #length
                        ));
                        #failure
                    }
                }
            };
        })
    }

    fn empty_value(&self) -> Option<TokenStream> {
        match self.ty {
            TypeRef::Optional(_) => Some(quote! { None }),
            TypeRef::Sequence(_) => Some(quote! { Vec::new() }),
            _ => None,
        }
    }
}

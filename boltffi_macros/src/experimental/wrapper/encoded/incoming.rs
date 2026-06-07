use boltffi_binding::CodecNode;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{error::Error, rust_api, wrapper::names};

pub struct Value<'a> {
    root: &'a CodecNode,
}

pub struct Input<'a> {
    target: &'a rust_api::DecodeTarget,
    binding: &'a Ident,
    pointer: &'a Ident,
    length: &'a Ident,
    failure: &'a TokenStream,
}

impl<'a> Value<'a> {
    pub const fn new(root: &'a CodecNode) -> Self {
        Self { root }
    }

    pub fn decode(&self, input: Input<'_>) -> Result<TokenStream, Error> {
        super::require_runtime_wire(self.root)?;
        match input.target.borrow() {
            rust_api::DecodeBorrow::Owned => {
                input.owned(self.root, input.target.owned(), input.binding, false)
            }
            borrow => input.reference(self.root, borrow),
        }
    }
}

impl<'a> Input<'a> {
    pub const fn new(
        target: &'a rust_api::DecodeTarget,
        binding: &'a Ident,
        pointer: &'a Ident,
        length: &'a Ident,
        failure: &'a TokenStream,
    ) -> Self {
        Self {
            target,
            binding,
            pointer,
            length,
            failure,
        }
    }

    fn reference(
        &self,
        root: &CodecNode,
        borrow: rust_api::DecodeBorrow,
    ) -> Result<TokenStream, Error> {
        let storage = names::Parameter::new(self.binding).storage();
        let owned = self.owned(root, self.target.owned(), &storage, borrow.mutable())?;
        let binding = self.binding;
        let borrow = self.borrow(&storage, borrow)?;
        Ok(quote! {
            #owned
            let #binding = #borrow;
        })
    }

    fn owned(
        &self,
        root: &CodecNode,
        rust_type: &Type,
        binding: &Ident,
        mutable: bool,
    ) -> Result<TokenStream, Error> {
        let pointer = self.pointer;
        let length = self.length;
        let failure = self.failure;
        let mutability = mutable.then(|| quote! { mut });
        let turbofish = self.decode_type(root, rust_type);
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
                match ::boltffi::__private::wire::decode::<#turbofish>(__boltffi_bytes) {
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

    fn decode_type(&self, root: &CodecNode, rust_type: &Type) -> Type {
        match root {
            CodecNode::Bytes => syn::parse_quote! { Vec<u8> },
            _ => rust_type.clone(),
        }
    }

    fn borrow(
        &self,
        storage: &Ident,
        borrow: rust_api::DecodeBorrow,
    ) -> Result<TokenStream, Error> {
        match borrow {
            rust_api::DecodeBorrow::Owned => Err(Error::SourceSyntaxMismatch(
                "owned encoded parameter does not borrow decoded storage",
            )),
            rust_api::DecodeBorrow::Value { mutable: false } => Ok(quote! { &#storage }),
            rust_api::DecodeBorrow::Value { mutable: true } => Ok(quote! { &mut #storage }),
            rust_api::DecodeBorrow::Slice { mutable: false } => Ok(quote! { #storage.as_slice() }),
            rust_api::DecodeBorrow::Slice { mutable: true } => {
                Ok(quote! { #storage.as_mut_slice() })
            }
            rust_api::DecodeBorrow::Str { mutable: false } => Ok(quote! { #storage.as_str() }),
            rust_api::DecodeBorrow::Str { mutable: true } => Ok(quote! { #storage.as_mut_str() }),
        }
    }
}

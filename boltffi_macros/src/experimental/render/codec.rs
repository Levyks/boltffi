use boltffi_binding::{CodecNode, Receive};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use super::local;
use crate::experimental::error::Error;

pub struct EncodedValue<'a> {
    root: &'a CodecNode,
}

pub struct DecodeInput<'a> {
    receive: Receive,
    syntax: &'a Type,
    binding: &'a Ident,
    pointer: &'a Ident,
    length: &'a Ident,
    failure: &'a TokenStream,
}

struct Reference<'a> {
    ty: &'a Type,
    mutable: bool,
}

impl<'a> EncodedValue<'a> {
    pub const fn new(root: &'a CodecNode) -> Self {
        Self { root }
    }

    pub fn buffer(&self, value: TokenStream) -> Result<TokenStream, Error> {
        self.require_runtime_wire()?;
        Ok(quote! { ::boltffi::__private::FfiBuf::wire_encode(&#value) })
    }

    pub fn conversion(&self, input: DecodeInput<'_>) -> Result<TokenStream, Error> {
        self.require_runtime_wire()?;
        match input.receive {
            Receive::ByValue => input.owned(self.root, input.syntax, input.binding, false),
            Receive::ByRef => input.reference(self.root, false),
            Receive::ByMutRef => input.reference(self.root, true),
            _ => Err(Error::UnsupportedExpansion(
                "unknown encoded parameter receive mode",
            )),
        }
    }

    fn require_runtime_wire(&self) -> Result<(), Error> {
        if Self::uses_runtime_wire(self.root) {
            Ok(())
        } else {
            Err(Error::UnsupportedExpansion("codec node"))
        }
    }

    fn uses_runtime_wire(root: &CodecNode) -> bool {
        match root {
            CodecNode::Primitive(_)
            | CodecNode::String
            | CodecNode::Bytes
            | CodecNode::DirectRecord(_)
            | CodecNode::EncodedRecord(_)
            | CodecNode::CStyleEnum(_)
            | CodecNode::DataEnum(_)
            | CodecNode::Custom(_) => true,
            CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
                Self::uses_runtime_wire(inner)
            }
            CodecNode::Result { ok, err } => {
                Self::uses_runtime_wire(ok) && Self::uses_runtime_wire(err)
            }
            CodecNode::Tuple(_)
            | CodecNode::Map { .. }
            | CodecNode::ClassHandle(_)
            | CodecNode::CallbackHandle(_)
            | _ => false,
        }
    }
}

impl<'a> DecodeInput<'a> {
    pub const fn new(
        receive: Receive,
        syntax: &'a Type,
        binding: &'a Ident,
        pointer: &'a Ident,
        length: &'a Ident,
        failure: &'a TokenStream,
    ) -> Self {
        Self {
            receive,
            syntax,
            binding,
            pointer,
            length,
            failure,
        }
    }

    fn reference(&self, root: &CodecNode, mutable: bool) -> Result<TokenStream, Error> {
        let reference = Reference::parse(self.syntax, mutable)?;
        let storage = local::Parameter::new(self.binding).storage();
        let owned_type = self.owned_reference_type(root, &reference);
        let owned = self.owned(root, &owned_type, &storage, mutable)?;
        let binding = self.binding;
        let borrow = reference.borrow(&storage);
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

    fn owned_reference_type(&self, root: &CodecNode, reference: &Reference<'_>) -> Type {
        match root {
            CodecNode::String => syn::parse_quote! { String },
            CodecNode::Bytes => syn::parse_quote! { Vec<u8> },
            _ => reference.ty.clone(),
        }
    }

    fn decode_type(&self, root: &CodecNode, rust_type: &Type) -> Type {
        match root {
            CodecNode::Bytes => syn::parse_quote! { Vec<u8> },
            _ => rust_type.clone(),
        }
    }
}

impl<'a> Reference<'a> {
    fn parse(ty: &'a Type, mutable: bool) -> Result<Self, Error> {
        let Type::Reference(reference) = ty else {
            return Err(Error::SourceSyntaxMismatch(
                "encoded reference parameter syntax does not match binding receive mode",
            ));
        };
        if reference.mutability.is_some() != mutable {
            return Err(Error::SourceSyntaxMismatch(
                "encoded reference parameter syntax does not match binding receive mode",
            ));
        }
        Ok(Self {
            ty: reference.elem.as_ref(),
            mutable,
        })
    }

    fn borrow(&self, storage: &Ident) -> TokenStream {
        match (self.mutable, self.ty) {
            (false, Type::Slice(_)) => quote! { #storage.as_slice() },
            (true, Type::Slice(_)) => quote! { #storage.as_mut_slice() },
            (false, ty) if Self::is_str(ty) => quote! { #storage.as_str() },
            (true, ty) if Self::is_str(ty) => quote! { #storage.as_mut_str() },
            (false, _) => quote! { &#storage },
            (true, _) => quote! { &mut #storage },
        }
    }

    fn is_str(ty: &Type) -> bool {
        let Type::Path(path) = ty else {
            return false;
        };
        path.path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "str")
    }
}

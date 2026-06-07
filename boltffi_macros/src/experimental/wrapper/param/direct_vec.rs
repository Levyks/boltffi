use boltffi_binding::{Primitive, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    target::Target,
    wrapper::{self, Render, names},
};

use super::Tokens;

pub struct Renderer;

pub struct Input<'binding> {
    element: &'binding TypeRef,
    rust_element: Type,
    ident: Ident,
    failure: TokenStream,
}

impl<'binding> Input<'binding> {
    pub fn new(
        element: &'binding TypeRef,
        rust_element: Type,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            element,
            rust_element,
            ident,
            failure,
        }
    }
}

impl<'binding, S> Render<S, Input<'binding>> for Renderer
where
    S: Target,
    for<'ty> wrapper::type_ref::Renderer: Render<S, &'ty TypeRef, Output = TokenStream>,
{
    type Output = Tokens;

    fn render(self, input: Input<'binding>) -> Result<Self::Output, Error> {
        match input.element {
            TypeRef::Primitive(primitive) => {
                PrimitiveVec::new(*primitive, input.ident).tokens::<S>()
            }
            TypeRef::Record(_) => {
                let rust_element = input.rust_element;
                PassableVec::new(rust_element, input.ident, input.failure).tokens()
            }
            _ => Err(Error::UnsupportedExpansion("direct-vector element")),
        }
    }
}

struct PrimitiveVec {
    primitive: Primitive,
    ident: Ident,
}

impl PrimitiveVec {
    fn new(primitive: Primitive, ident: Ident) -> Self {
        Self { primitive, ident }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        S: Target,
        for<'ty> wrapper::type_ref::Renderer: Render<S, &'ty TypeRef, Output = TokenStream>,
    {
        let ident = &self.ident;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let element = TypeRef::Primitive(self.primitive);
        let element_type = <wrapper::type_ref::Renderer as Render<S, &TypeRef>>::render(
            wrapper::type_ref::Renderer,
            &element,
        )?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![
                quote! { #pointer: *const #element_type },
                quote! { #length: usize },
            ],
            ffi_parameter_types: vec![quote! { *const #element_type }, quote! { usize }],
            conversions: vec![quote! {
                let #ident: Vec<#element_type> = if #pointer.is_null() {
                    Vec::new()
                } else {
                    unsafe { ::core::slice::from_raw_parts(#pointer, #length) }.to_vec()
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

struct PassableVec {
    element: Type,
    ident: Ident,
    failure: TokenStream,
}

impl PassableVec {
    fn new(element: Type, ident: Ident, failure: TokenStream) -> Self {
        Self {
            element,
            ident,
            failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let element = &self.element;
        let ident = &self.ident;
        let failure = self.failure;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            conversions: vec![quote! {
                let #ident: Vec<#element> = if #pointer.is_null() {
                    Vec::new()
                } else {
                    let raw_byte_len = #length;
                    let element_size = ::core::mem::size_of::<<#element as ::boltffi::__private::Passable>::In>();
                    if raw_byte_len % element_size == 0 {
                        unsafe {
                            <#element as ::boltffi::__private::VecTransport>::unpack_vec(
                                #pointer,
                                raw_byte_len
                            )
                        }
                    } else {
                        ::boltffi::__private::set_last_error(format!(
                            "invalid byte length {} for Vec<{}>: not divisible by element size {}",
                            raw_byte_len,
                            ::core::any::type_name::<#element>(),
                            element_size
                        ));
                        #failure
                    }
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

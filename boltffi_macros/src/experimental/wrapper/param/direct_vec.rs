use boltffi_binding::{DirectVectorElementType, Primitive};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    surface::RenderSurface,
    wrapper::{self, Render, names},
};

use super::Tokens;

pub struct Renderer;

pub struct Input {
    element: DirectVectorElementType,
    rust_element: Type,
    ident: Ident,
    failure: TokenStream,
}

impl Input {
    pub fn new(
        element: &DirectVectorElementType,
        rust_element: Type,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            element: element.clone(),
            rust_element,
            ident,
            failure,
        }
    }
}

impl<S> Render<S, Input> for Renderer
where
    S: RenderSurface,
{
    type Output = Tokens;

    fn render(self, input: Input) -> Result<Self::Output, Error> {
        match &input.element {
            DirectVectorElementType::Primitive(primitive) => {
                PrimitiveVec::new(primitive.primitive(), input.ident).tokens()
            }
            DirectVectorElementType::Record(_) => {
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

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let locals = names::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let element_type = wrapper::type_ref::Renderer.primitive(self.primitive)?;
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

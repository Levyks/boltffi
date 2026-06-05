use boltffi_binding::{Primitive, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{GenericArgument, PatType, PathArguments, Type};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, local},
    target::Target,
};

use super::Tokens;

pub struct Rule;

pub struct Input<'binding, 'syntax> {
    element: &'binding TypeRef,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: TokenStream,
}

impl<'binding, 'syntax> Input<'binding, 'syntax> {
    pub fn new(
        element: &'binding TypeRef,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            element,
            syntax,
            ident,
            failure,
        }
    }

    fn rust_element(&self) -> Result<&'syntax Type, Error> {
        let Type::Path(path) = self.syntax.ty.as_ref() else {
            return Err(Error::SourceSyntaxMismatch(
                "direct-vector parameter requires Vec<T> source syntax",
            ));
        };
        let Some(segment) = path.path.segments.last() else {
            return Err(Error::SourceSyntaxMismatch(
                "direct-vector parameter requires Vec<T> source syntax",
            ));
        };
        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return Err(Error::SourceSyntaxMismatch(
                "direct-vector parameter requires Vec<T> source syntax",
            ));
        };
        arguments
            .args
            .iter()
            .find_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .ok_or(Error::SourceSyntaxMismatch(
                "direct-vector parameter requires Vec<T> source syntax",
            ))
    }
}

impl<'binding, 'syntax, S> RenderRule<S, Input<'binding, 'syntax>> for Rule
where
    S: Target,
    for<'ty> render::type_ref::Rule: RenderRule<S, &'ty TypeRef, Output = TokenStream>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax>) -> Result<Self::Output, Error> {
        match input.element {
            TypeRef::Primitive(primitive) => {
                PrimitiveVec::new(*primitive, input.ident).tokens::<S>()
            }
            TypeRef::Record(_) => {
                PassableVec::new(input.rust_element()?, input.ident, input.failure).tokens()
            }
            _ => Err(Error::UnsupportedExpansion("direct-vector element")),
        }
    }
}

struct PrimitiveVec<'a> {
    primitive: Primitive,
    ident: &'a syn::Ident,
}

impl<'a> PrimitiveVec<'a> {
    fn new(primitive: Primitive, ident: &'a syn::Ident) -> Self {
        Self { primitive, ident }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        S: Target,
        for<'ty> render::type_ref::Rule: RenderRule<S, &'ty TypeRef, Output = TokenStream>,
    {
        let ident = self.ident;
        let locals = local::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let element = TypeRef::Primitive(self.primitive);
        let element_type = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
            render::type_ref::Rule,
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

struct PassableVec<'a> {
    element: &'a Type,
    ident: &'a syn::Ident,
    failure: TokenStream,
}

impl<'a> PassableVec<'a> {
    fn new(element: &'a Type, ident: &'a syn::Ident, failure: TokenStream) -> Self {
        Self {
            element,
            ident,
            failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let element = self.element;
        let ident = self.ident;
        let failure = self.failure;
        let locals = local::Parameter::new(ident);
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

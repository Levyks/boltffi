use boltffi_binding::{Native, Primitive, Wasm32};
use quote::quote;
use syn::PatType;

use crate::experimental::{
    error::Error,
    render::{Rule as RenderRule, local},
};

use super::Tokens;

pub struct Rule;

pub struct Input<'syntax> {
    primitive: Primitive,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: proc_macro2::TokenStream,
}

impl<'syntax> Input<'syntax> {
    pub fn new(
        primitive: Primitive,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: proc_macro2::TokenStream,
    ) -> Self {
        Self {
            primitive,
            syntax,
            ident,
            failure,
        }
    }
}

impl<'syntax> RenderRule<Native, Input<'syntax>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'syntax>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        let locals = local::Parameter::new(ident);
        let pointer = locals.pointer();
        let length = locals.length();
        let rust_type = input.syntax.ty.as_ref();
        let failure = input.failure;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            conversions: vec![quote! {
                let #ident: #rust_type = if #pointer.is_null() {
                    None
                } else {
                    match ::boltffi::__private::wire::decode(unsafe {
                        ::core::slice::from_raw_parts(#pointer, #length)
                    }) {
                        Ok(value) => value,
                        Err(error) => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: invalid optional scalar payload: {} (buf_len={})",
                                stringify!(#ident),
                                error,
                                #length
                            ));
                            #failure
                        }
                    }
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

impl<'syntax> RenderRule<Wasm32, Input<'syntax>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'syntax>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        let rust_type = input.syntax.ty.as_ref();
        let value = Scalar::new(input.primitive, ident).some_value()?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: f64 }],
            ffi_parameter_types: vec![quote! { f64 }],
            conversions: vec![quote! {
                let #ident: #rust_type = if #ident.is_nan() {
                    None
                } else {
                    Some(#value)
                };
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

struct Scalar<'a> {
    primitive: Primitive,
    value: &'a syn::Ident,
}

impl<'a> Scalar<'a> {
    fn new(primitive: Primitive, value: &'a syn::Ident) -> Self {
        Self { primitive, value }
    }

    fn some_value(self) -> Result<proc_macro2::TokenStream, Error> {
        let value = self.value;
        Ok(match self.primitive {
            Primitive::Bool => quote! { #value != 0.0 },
            Primitive::F64 => quote! { #value },
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize
            | Primitive::F32 => quote! { #value as _ },
            _ => return Err(Error::UnsupportedExpansion("scalar option primitive")),
        })
    }
}

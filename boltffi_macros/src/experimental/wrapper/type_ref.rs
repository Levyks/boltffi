use boltffi_binding::{Primitive, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{error::Error, target::Target, wrapper::Render};

pub struct Renderer;

impl<S: Target> Render<S, &TypeRef> for Renderer {
    type Output = TokenStream;

    fn render(self, ty: &TypeRef) -> Result<Self::Output, Error> {
        match ty {
            TypeRef::Primitive(primitive) => self.primitive(*primitive),
            _ => Err(Error::UnsupportedExpansion("non-primitive type")),
        }
    }
}

impl Renderer {
    fn primitive(self, primitive: Primitive) -> Result<TokenStream, Error> {
        Ok(match primitive {
            Primitive::Bool => quote! { bool },
            Primitive::I8 => quote! { i8 },
            Primitive::U8 => quote! { u8 },
            Primitive::I16 => quote! { i16 },
            Primitive::U16 => quote! { u16 },
            Primitive::I32 => quote! { i32 },
            Primitive::U32 => quote! { u32 },
            Primitive::I64 => quote! { i64 },
            Primitive::U64 => quote! { u64 },
            Primitive::ISize => quote! { isize },
            Primitive::USize => quote! { usize },
            Primitive::F32 => quote! { f32 },
            Primitive::F64 => quote! { f64 },
            _ => return Err(Error::UnsupportedExpansion("unknown primitive")),
        })
    }
}

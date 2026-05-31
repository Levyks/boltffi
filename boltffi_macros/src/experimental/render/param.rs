use boltffi_binding::{IncomingParam, IntoRust, ParamDecl, ParamPlan, TypeRef};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule;

pub struct Tokens {
    ffi_param: TokenStream,
    call_arg: TokenStream,
}

impl Tokens {
    pub fn ffi_param(&self) -> &TokenStream {
        &self.ffi_param
    }

    pub fn call_arg(&self) -> &TokenStream {
        &self.call_arg
    }
}

impl<'a, S: Target> RenderRule<S, &'a ParamDecl<S, IntoRust>> for Rule {
    type Output = Tokens;

    fn apply(self, param: &'a ParamDecl<S, IntoRust>) -> Result<Self::Output, Error> {
        let ident = format_ident!("{}", param.name().as_path_string().replace("::", "_"));
        let ty = match param.payload() {
            IncomingParam::Value(ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                ..
            }) => {
                let ty = TypeRef::Primitive(*primitive);
                <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?
            }
            IncomingParam::Value(_) => return Err(Error::UnsupportedExpansion("non-direct param")),
            IncomingParam::Closure(_) => {
                return Err(Error::UnsupportedExpansion("closure param"));
            }
        };

        Ok(Tokens {
            ffi_param: quote! { #ident: #ty },
            call_arg: quote! { #ident },
        })
    }
}

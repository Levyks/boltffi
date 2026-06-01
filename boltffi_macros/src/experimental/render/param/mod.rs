use boltffi_binding::{IncomingParam, IntoRust, ParamDecl, ParamPlan, Receive, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Pat, PatType, Type};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

mod encoded;

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    param: &'binding ParamDecl<S, IntoRust>,
    syntax: &'syntax PatType,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(param: &'binding ParamDecl<S, IntoRust>, syntax: &'syntax PatType) -> Self {
        Self { param, syntax }
    }
}

pub struct Tokens {
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    argument: TokenStream,
}

impl Tokens {
    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn conversions(&self) -> &[TokenStream] {
        &self.conversions
    }

    pub fn argument(&self) -> &TokenStream {
        &self.argument
    }
}

impl<'binding, 'syntax, S> RenderRule<S, Input<'binding, 'syntax, S>> for Rule
where
    S: Target,
    encoded::Rule: RenderRule<S, encoded::Input<'binding, 'syntax, S>, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, S>) -> Result<Self::Output, Error> {
        let ident = Self::syntax_ident(input.syntax)?;
        match input.param.payload() {
            IncomingParam::Value(ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                receive,
            }) => {
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?;
                let argument = Self::direct_argument(*receive, ident)?;
                Ok(Tokens {
                    ffi_parameters: vec![quote! { #ident: #ffi_type }],
                    conversions: Vec::new(),
                    argument,
                })
            }
            IncomingParam::Value(ParamPlan::Direct { receive, .. }) => {
                let rust_type = Self::rust_type(input.syntax, *receive)?;
                let argument = Self::direct_argument(*receive, ident)?;
                Ok(Tokens {
                    ffi_parameters: vec![quote! {
                        #ident: <#rust_type as ::boltffi::__private::Passable>::In
                    }],
                    conversions: vec![quote! {
                        let #ident: #rust_type = unsafe {
                            <#rust_type as ::boltffi::__private::Passable>::unpack(#ident)
                        };
                    }],
                    argument,
                })
            }
            IncomingParam::Value(ParamPlan::Encoded {
                ty, shape, receive, ..
            }) => <encoded::Rule as RenderRule<S, _>>::apply(
                encoded::Rule,
                encoded::Input::new(ty, *shape, *receive, ident),
            ),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("non-direct param")),
            IncomingParam::Closure(_) => Err(Error::UnsupportedExpansion("closure param")),
        }
    }
}

impl Rule {
    fn syntax_ident(syntax: &PatType) -> Result<&syn::Ident, Error> {
        match syntax.pat.as_ref() {
            Pat::Ident(ident) => Ok(&ident.ident),
            _ => Err(Error::SourceSyntaxMismatch(
                "function parameter syntax is not a plain identifier",
            )),
        }
    }

    fn rust_type(syntax: &PatType, receive: Receive) -> Result<&Type, Error> {
        match (receive, syntax.ty.as_ref()) {
            (Receive::ByValue, ty) => Ok(ty),
            (Receive::ByRef, Type::Reference(reference)) if reference.mutability.is_none() => {
                Ok(reference.elem.as_ref())
            }
            (Receive::ByRef, _) => Err(Error::SourceSyntaxMismatch(
                "shared-reference parameter syntax does not match binding receive mode",
            )),
            (Receive::ByMutRef, _) => Err(Error::UnsupportedExpansion(
                "mutable-reference direct parameter",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct parameter receive mode",
            )),
        }
    }

    fn direct_argument(receive: Receive, ident: &syn::Ident) -> Result<TokenStream, Error> {
        match receive {
            Receive::ByValue => Ok(quote! { #ident }),
            Receive::ByRef => Ok(quote! { &#ident }),
            Receive::ByMutRef => Err(Error::UnsupportedExpansion(
                "mutable-reference direct parameter",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct parameter receive mode",
            )),
        }
    }
}

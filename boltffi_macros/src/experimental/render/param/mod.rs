use boltffi_binding::{IncomingParam, IntoRust, ParamDecl, ParamPlan};
use proc_macro2::TokenStream;
use syn::{Pat, PatType};

use crate::experimental::{error::Error, render::Rule as RenderRule, target::Target};

pub mod closure;
mod direct;
mod direct_vec;
mod encoded;
mod handle;
mod scalar_option;

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    param: &'binding ParamDecl<S, IntoRust>,
    syntax: &'syntax PatType,
    failure: TokenStream,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        param: &'binding ParamDecl<S, IntoRust>,
        syntax: &'syntax PatType,
        failure: TokenStream,
    ) -> Self {
        Self {
            param,
            syntax,
            failure,
        }
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    argument: TokenStream,
}

impl Tokens {
    pub fn items(&self) -> &[TokenStream] {
        &self.items
    }

    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn ffi_parameter_types(&self) -> &[TokenStream] {
        &self.ffi_parameter_types
    }

    pub fn conversions(&self) -> &[TokenStream] {
        &self.conversions
    }

    pub fn writebacks(&self) -> &[TokenStream] {
        &self.writebacks
    }

    pub fn argument(&self) -> &TokenStream {
        &self.argument
    }
}

impl<'binding, 'syntax, S> RenderRule<S, Input<'binding, 'syntax, S>> for Rule
where
    S: Target,
    direct::Rule: RenderRule<S, direct::Input<'binding, 'syntax>, Output = Tokens>,
    direct_vec::Rule: RenderRule<S, direct_vec::Input<'binding, 'syntax>, Output = Tokens>,
    closure::Rule: RenderRule<S, closure::Input<'binding, 'syntax, S>, Output = Tokens>,
    encoded::Rule: RenderRule<S, encoded::Input<'binding, 'syntax, S>, Output = Tokens>,
    handle::Rule:
        RenderRule<S, handle::Input<'binding, 'syntax, S::HandleCarrier>, Output = Tokens>,
    scalar_option::Rule: RenderRule<S, scalar_option::Input<'syntax>, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, S>) -> Result<Self::Output, Error> {
        let ident = Self::syntax_ident(input.syntax)?;
        match input.param.payload() {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                <direct::Rule as RenderRule<S, _>>::apply(
                    direct::Rule,
                    direct::Input::new(ty, *receive, input.syntax, ident, input.failure),
                )
            }
            IncomingParam::Value(ParamPlan::Encoded {
                ty, shape, receive, ..
            }) => <encoded::Rule as RenderRule<S, _>>::apply(
                encoded::Rule,
                encoded::Input::new(ty, *shape, *receive, input.syntax, ident, input.failure),
            ),
            IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                <scalar_option::Rule as RenderRule<S, _>>::apply(
                    scalar_option::Rule,
                    scalar_option::Input::new(*primitive, input.syntax, ident, input.failure),
                )
            }
            IncomingParam::Value(ParamPlan::DirectVec { element }) => {
                <direct_vec::Rule as RenderRule<S, _>>::apply(
                    direct_vec::Rule,
                    direct_vec::Input::new(element, input.syntax, ident, input.failure),
                )
            }
            IncomingParam::Value(ParamPlan::Handle {
                target,
                carrier,
                presence,
                receive,
            }) => <handle::Rule as RenderRule<S, _>>::apply(
                handle::Rule,
                handle::Input::new(
                    target,
                    *carrier,
                    *presence,
                    *receive,
                    input.syntax,
                    ident,
                    input.failure,
                ),
            ),
            IncomingParam::Closure(closure) => <closure::Rule as RenderRule<S, _>>::apply(
                closure::Rule,
                closure::Input::new(closure, input.syntax.ty.as_ref(), ident, input.failure),
            ),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("unknown parameter plan")),
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
}

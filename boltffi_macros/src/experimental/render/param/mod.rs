use boltffi_binding::{IncomingParam, IntoRust, ParamDecl, ParamPlan};
use proc_macro2::TokenStream;

use crate::experimental::{
    error::Error,
    render::{Rule as RenderRule, callable::signature},
    target::Target,
};

pub mod closure;
mod direct;
mod direct_vec;
mod encoded;
mod handle;
mod scalar_option;

pub struct Rule;

pub struct Input<'binding, S: Target> {
    param: &'binding ParamDecl<S, IntoRust>,
    source: signature::Parameter<'binding>,
    failure: TokenStream,
}

impl<'binding, S: Target> Input<'binding, S> {
    pub fn new(
        param: &'binding ParamDecl<S, IntoRust>,
        source: signature::Parameter<'binding>,
        failure: TokenStream,
    ) -> Self {
        Self {
            param,
            source,
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

impl<'binding, S> RenderRule<S, Input<'binding, S>> for Rule
where
    S: Target,
    direct::Rule: RenderRule<S, direct::Input<'binding>, Output = Tokens>,
    direct_vec::Rule: RenderRule<S, direct_vec::Input<'binding>, Output = Tokens>,
    closure::Rule: RenderRule<S, closure::Input<'binding, S>, Output = Tokens>,
    encoded::Rule: RenderRule<S, encoded::Input<'binding, S>, Output = Tokens>,
    handle::Rule: RenderRule<S, handle::Input<'binding, S::HandleCarrier>, Output = Tokens>,
    scalar_option::Rule: RenderRule<S, scalar_option::Input, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, S>) -> Result<Self::Output, Error> {
        let ident = input.source.ident()?;
        match input.param.payload() {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                <direct::Rule as RenderRule<S, _>>::apply(
                    direct::Rule,
                    direct::Input::new(
                        ty,
                        *receive,
                        input.source.written_type()?,
                        ident,
                        input.failure,
                    ),
                )
            }
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                shape,
                receive,
                ..
            }) => <encoded::Rule as RenderRule<S, _>>::apply(
                encoded::Rule,
                encoded::Input::new(
                    codec,
                    *shape,
                    *receive,
                    input.source.value_type(*receive)?,
                    ident,
                    input.failure,
                ),
            ),
            IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                input.source.scalar_option(*primitive)?;
                <scalar_option::Rule as RenderRule<S, _>>::apply(
                    scalar_option::Rule,
                    scalar_option::Input::new(
                        *primitive,
                        input.source.written_type()?,
                        ident,
                        input.failure,
                    ),
                )
            }
            IncomingParam::Value(ParamPlan::DirectVec { element }) => {
                <direct_vec::Rule as RenderRule<S, _>>::apply(
                    direct_vec::Rule,
                    direct_vec::Input::new(
                        element,
                        input.source.direct_vec_element_type()?,
                        ident,
                        input.failure,
                    ),
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
                    handle::Plan::new(target, *carrier, *presence, *receive),
                    input.source,
                    ident,
                    input.failure,
                ),
            ),
            IncomingParam::Closure(closure) => <closure::Rule as RenderRule<S, _>>::apply(
                closure::Rule,
                closure::Input::new(
                    closure,
                    input.source.closure(closure.presence())?,
                    input.source.written_type()?,
                    ident,
                    input.failure,
                ),
            ),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("unknown parameter plan")),
        }
    }
}

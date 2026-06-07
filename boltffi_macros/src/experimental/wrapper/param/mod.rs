use boltffi_binding::{IncomingParam, IntoRust, ParamDecl, ParamPlan};
use proc_macro2::TokenStream;

use crate::experimental::{
    error::Error, expansion::Expansion, rust_api, target::Target, wrapper::Render,
};

pub mod closure;
mod direct;
mod direct_vec;
mod encoded;
mod handle;
mod scalar_option;

pub struct Renderer;

pub fn requires_failure_return<S: Target>(param: &ParamDecl<S, IntoRust>) -> bool {
    match param.payload() {
        IncomingParam::Value(ParamPlan::Direct { ty, .. }) => {
            S::direct_record_params_use_pointers()
                && matches!(ty, boltffi_binding::TypeRef::Record(_))
        }
        IncomingParam::Value(ParamPlan::Encoded { .. })
        | IncomingParam::Value(ParamPlan::Handle { .. })
        | IncomingParam::Value(ParamPlan::ScalarOption { .. })
        | IncomingParam::Value(ParamPlan::DirectVec { .. })
        | IncomingParam::Closure(_) => true,
        IncomingParam::Value(_) => true,
    }
}

pub struct Input<'context, 'binding, S: Target> {
    param: &'binding ParamDecl<S, IntoRust>,
    source: rust_api::Parameter<'binding>,
    failure: TokenStream,
    expansion: &'context Expansion<'binding, S>,
}

impl<'context, 'binding, S: Target> Input<'context, 'binding, S> {
    pub fn new(
        param: &'binding ParamDecl<S, IntoRust>,
        source: rust_api::Parameter<'binding>,
        failure: TokenStream,
        expansion: &'context Expansion<'binding, S>,
    ) -> Self {
        Self {
            param,
            source,
            failure,
            expansion,
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

impl<'context, 'binding, S> Render<S, Input<'context, 'binding, S>> for Renderer
where
    S: Target,
    direct::Renderer: Render<S, direct::Input<'binding>, Output = Tokens>,
    direct_vec::Renderer: Render<S, direct_vec::Input<'binding>, Output = Tokens>,
    closure::Renderer: Render<S, closure::Input<'context, 'binding, S>, Output = Tokens>,
    encoded::Renderer: Render<S, encoded::Input<'context, 'binding, S>, Output = Tokens>,
    handle::Renderer: Render<S, handle::Input<'binding, S::HandleCarrier>, Output = Tokens>,
    scalar_option::Renderer: Render<S, scalar_option::Input, Output = Tokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'context, 'binding, S>) -> Result<Self::Output, Error> {
        let ident = input.source.ident()?;
        match input.param.payload() {
            IncomingParam::Value(ParamPlan::Direct { ty, receive }) => {
                <direct::Renderer as Render<S, _>>::render(
                    direct::Renderer,
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
            }) => <encoded::Renderer as Render<S, _>>::render(
                encoded::Renderer,
                encoded::Input::new(
                    codec,
                    *shape,
                    input.source.decode_target(*receive)?,
                    ident,
                    input.failure,
                    input.expansion,
                ),
            ),
            IncomingParam::Value(ParamPlan::ScalarOption { primitive }) => {
                input.source.scalar_option(*primitive)?;
                <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Input::new(
                        *primitive,
                        input.source.written_type()?,
                        ident,
                        input.failure,
                    ),
                )
            }
            IncomingParam::Value(ParamPlan::DirectVec { element }) => {
                <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
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
            }) => <handle::Renderer as Render<S, _>>::render(
                handle::Renderer,
                handle::Input::new(
                    handle::Plan::new(target, *carrier, *presence, *receive),
                    input.source,
                    ident,
                    input.failure,
                ),
            ),
            IncomingParam::Closure(closure) => <closure::Renderer as Render<S, _>>::render(
                closure::Renderer,
                closure::Input::new(
                    closure,
                    input.source.closure(closure.presence())?,
                    ident,
                    input.failure,
                    input.expansion,
                ),
            ),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("unknown parameter plan")),
        }
    }
}

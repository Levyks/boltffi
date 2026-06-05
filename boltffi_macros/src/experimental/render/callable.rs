use boltffi_binding::{ExecutionDecl, ExportedCallable};
use proc_macro2::TokenStream;
use syn::PatType;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule;
pub struct Parameters;

pub struct Input<'binding, 'params, 'syntax, S: Target> {
    callable: &'binding ExportedCallable<S>,
    params: &'params [&'syntax PatType],
    failure: TokenStream,
}

impl<'binding, 'params, 'syntax, S: Target> Input<'binding, 'params, 'syntax, S> {
    pub fn new(
        callable: &'binding ExportedCallable<S>,
        params: &'params [&'syntax PatType],
        failure: TokenStream,
    ) -> Self {
        Self {
            callable,
            params,
            failure,
        }
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

impl Tokens {
    pub fn items(&self) -> &[TokenStream] {
        &self.items
    }

    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn conversions(&self) -> &[TokenStream] {
        &self.conversions
    }

    pub fn writebacks(&self) -> &[TokenStream] {
        &self.writebacks
    }

    pub fn arguments(&self) -> &[TokenStream] {
        &self.arguments
    }
}

impl<'binding, 'params, 'syntax, S> RenderRule<S, Input<'binding, 'params, 'syntax, S>>
    for Parameters
where
    S: Target,
    render::param::Rule:
        RenderRule<S, render::param::Input<'binding, 'syntax, S>, Output = render::param::Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'params, 'syntax, S>) -> Result<Self::Output, Error> {
        let callable = input.callable;
        if callable.params().len() != input.params.len() {
            return Err(Error::SourceSyntaxMismatch(
                "function syntax parameter count does not match binding parameter count",
            ));
        }

        let params = callable
            .params()
            .iter()
            .zip(input.params.iter().copied())
            .map(|(param, syntax)| {
                <render::param::Rule as RenderRule<S, _>>::apply(
                    render::param::Rule,
                    render::param::Input::new(param, syntax, input.failure.clone()),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let items = params
            .iter()
            .flat_map(|param| param.items().iter().cloned())
            .collect();
        let ffi_parameters = params
            .iter()
            .flat_map(|param| param.ffi_parameters().iter().cloned())
            .collect();
        let conversions = params
            .iter()
            .flat_map(|param| param.conversions().iter().cloned())
            .collect();
        let writebacks = params
            .iter()
            .flat_map(|param| param.writebacks().iter().cloned())
            .collect();
        let arguments = params
            .iter()
            .map(|param| param.argument().clone())
            .collect();

        Ok(Tokens {
            items,
            ffi_parameters,
            conversions,
            writebacks,
            arguments,
        })
    }
}

impl<'binding, 'params, 'syntax, S> RenderRule<S, Input<'binding, 'params, 'syntax, S>> for Rule
where
    S: Target,
    Parameters: RenderRule<S, Input<'binding, 'params, 'syntax, S>, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'params, 'syntax, S>) -> Result<Self::Output, Error> {
        match input.callable.execution() {
            ExecutionDecl::Synchronous(_) => {}
            ExecutionDecl::Asynchronous(_) => {
                return Err(Error::UnsupportedExpansion("async function"));
            }
            _ => return Err(Error::UnsupportedExpansion("unknown execution")),
        }

        <Parameters as RenderRule<S, _>>::apply(Parameters, input)
    }
}

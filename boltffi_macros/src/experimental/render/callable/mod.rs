use boltffi_binding::{ExecutionDecl, ExportedCallable};
use proc_macro2::TokenStream;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub mod signature;

pub struct Rule;
pub struct Parameters;

pub struct Input<'binding, S: Target> {
    callable: &'binding ExportedCallable<S>,
    source: signature::Callable<'binding>,
    failure: TokenStream,
}

impl<'binding, S: Target> Input<'binding, S> {
    pub fn new(
        callable: &'binding ExportedCallable<S>,
        source: signature::Callable<'binding>,
        failure: TokenStream,
    ) -> Self {
        Self {
            callable,
            source,
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

impl<'binding, S> RenderRule<S, Input<'binding, S>> for Parameters
where
    S: Target,
    render::param::Rule:
        RenderRule<S, render::param::Input<'binding, S>, Output = render::param::Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, S>) -> Result<Self::Output, Error> {
        let callable = input.callable;
        if callable.params().len() != input.source.parameters().len() {
            return Err(Error::SourceSyntaxMismatch(
                "source parameter count does not match binding parameter count",
            ));
        }

        let params = callable
            .params()
            .iter()
            .zip(input.source.parameters())
            .map(|(param, source)| {
                <render::param::Rule as RenderRule<S, _>>::apply(
                    render::param::Rule,
                    render::param::Input::new(
                        param,
                        signature::Parameter::new(source),
                        input.failure.clone(),
                    ),
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

impl<'binding, S> RenderRule<S, Input<'binding, S>> for Rule
where
    S: Target,
    Parameters: RenderRule<S, Input<'binding, S>, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, S>) -> Result<Self::Output, Error> {
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

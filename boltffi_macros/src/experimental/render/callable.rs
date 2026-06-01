use boltffi_binding::{ErrorDecl, ExecutionDecl, ExportedCallable};
use proc_macro2::TokenStream;
use syn::PatType;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    callable: &'binding ExportedCallable<S>,
    params: &'syntax [&'syntax PatType],
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        callable: &'binding ExportedCallable<S>,
        params: &'syntax [&'syntax PatType],
    ) -> Self {
        Self { callable, params }
    }
}

pub struct Tokens {
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

impl Tokens {
    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn conversions(&self) -> &[TokenStream] {
        &self.conversions
    }

    pub fn arguments(&self) -> &[TokenStream] {
        &self.arguments
    }
}

impl<'binding, 'syntax, S> RenderRule<S, Input<'binding, 'syntax, S>> for Rule
where
    S: Target,
    render::param::Rule:
        RenderRule<S, render::param::Input<'binding, 'syntax, S>, Output = render::param::Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, S>) -> Result<Self::Output, Error> {
        let callable = input.callable;
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => {}
            ExecutionDecl::Asynchronous(_) => {
                return Err(Error::UnsupportedExpansion("async function"));
            }
            _ => return Err(Error::UnsupportedExpansion("unknown execution")),
        }

        match callable.error() {
            ErrorDecl::None(_) => {}
            ErrorDecl::StatusViaReturnSlot { .. }
            | ErrorDecl::StatusViaOutPointer { .. }
            | ErrorDecl::EncodedViaReturnSlot { .. }
            | ErrorDecl::EncodedViaOutPointer { .. } => {
                return Err(Error::UnsupportedExpansion("fallible function"));
            }
            _ => return Err(Error::UnsupportedExpansion("unknown error channel")),
        }

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
                    render::param::Input::new(param, syntax),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ffi_parameters = params
            .iter()
            .flat_map(|param| param.ffi_parameters().iter().cloned())
            .collect();
        let conversions = params
            .iter()
            .flat_map(|param| param.conversions().iter().cloned())
            .collect();
        let arguments = params
            .iter()
            .map(|param| param.argument().clone())
            .collect();

        Ok(Tokens {
            ffi_parameters,
            conversions,
            arguments,
        })
    }
}

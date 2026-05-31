use boltffi_binding::{ErrorDecl, ExecutionDecl, ExportedCallable};
use proc_macro2::TokenStream;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule;

pub struct Tokens {
    ffi_params: Vec<TokenStream>,
    call_args: Vec<TokenStream>,
    return_type: TokenStream,
}

impl Tokens {
    pub fn ffi_params(&self) -> &[TokenStream] {
        &self.ffi_params
    }

    pub fn call_args(&self) -> &[TokenStream] {
        &self.call_args
    }

    pub fn return_type(&self) -> &TokenStream {
        &self.return_type
    }
}

impl<'a, S: Target> RenderRule<S, &'a ExportedCallable<S>> for Rule {
    type Output = Tokens;

    fn apply(self, callable: &'a ExportedCallable<S>) -> Result<Self::Output, Error> {
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

        let params = callable
            .params()
            .iter()
            .map(|param| {
                <render::param::Rule as RenderRule<S, _>>::apply(render::param::Rule, param)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ffi_params = params
            .iter()
            .map(|param| param.ffi_param().clone())
            .collect();
        let call_args = params
            .iter()
            .map(|param| param.call_arg().clone())
            .collect();
        let return_type = <render::returns::Rule as RenderRule<S, _>>::apply(
            render::returns::Rule,
            callable.returns(),
        )?;

        Ok(Tokens {
            ffi_params,
            call_args,
            return_type,
        })
    }
}

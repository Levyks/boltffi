use boltffi_binding::{ExecutionDecl, ExportedCallable, NativeSymbol};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{self, Render},
};

pub struct Renderer<'context, 'binding, S: Target> {
    symbol: &'binding NativeSymbol,
    callable: &'binding ExportedCallable<S>,
    source: rust_api::Callable<'binding>,
    rust_call: RustCall,
    receiver: ReceiverTokens,
    visibility: TokenStream,
    expansion: &'context Expansion<'binding, S>,
}

pub struct RustCall {
    owner: syn::Ident,
    target: RustCallTarget,
}

pub struct ReceiverTokens {
    ffi_parameters: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    requires_failure_return: bool,
}

enum RustCallTarget {
    Function(syn::Ident),
    Associated {
        owner: TokenStream,
        method: syn::Ident,
    },
    Method {
        receiver: syn::Ident,
        method: syn::Ident,
    },
}

impl<'context, 'binding, S> Renderer<'context, 'binding, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'context, 'binding, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'context, 'binding, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<S, wrapper::returns::Input<'context, 'binding, S>, Output = wrapper::returns::Tokens>,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'context, 'binding, S>, Output = TokenStream>,
{
    pub fn new(
        symbol: &'binding NativeSymbol,
        callable: &'binding ExportedCallable<S>,
        source: rust_api::Callable<'binding>,
        rust_call: RustCall,
        receiver: ReceiverTokens,
        visibility: TokenStream,
        expansion: &'context Expansion<'binding, S>,
    ) -> Self {
        Self {
            symbol,
            callable,
            source,
            rust_call,
            receiver,
            visibility,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error> {
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            return Err(Error::UnsupportedExpansion("async exported callable"));
        }

        let cfg = S::cfg_attr();
        let failure = self.failure()?;
        let wrapper_arguments = <wrapper::arguments::SyncRenderer as Render<S, _>>::render(
            wrapper::arguments::SyncRenderer,
            wrapper::arguments::Input::new(self.callable, self.source, failure, self.expansion),
        )?;
        let rust_call = self
            .rust_call
            .expression(wrapper_arguments.rust_arguments());
        let rust_invocation = wrapper::returns::RustInvocation::new(
            self.rust_call.owner,
            rust_call,
            self.receiver
                .conversions
                .into_iter()
                .chain(wrapper_arguments.conversions().iter().cloned())
                .collect(),
            wrapper_arguments
                .writebacks()
                .iter()
                .cloned()
                .chain(self.receiver.writebacks)
                .collect(),
        );
        let return_tokens = <wrapper::returns::Renderer as Render<S, _>>::render(
            wrapper::returns::Renderer,
            wrapper::returns::Input::new(
                self.callable.returns(),
                self.callable.error(),
                self.source.returns(),
                self.source.returns().written_type()?,
                rust_invocation,
                self.expansion,
            ),
        )?;
        let export_ident = format_ident!("{}", self.symbol.name().as_str());
        let ffi_parameters = self
            .receiver
            .ffi_parameters
            .iter()
            .chain(wrapper_arguments.ffi_parameters().iter())
            .chain(return_tokens.ffi_parameters().iter())
            .collect::<Vec<_>>();
        let argument_items = wrapper_arguments.items();
        let return_items = return_tokens.items();
        let return_type = return_tokens.return_type();
        let body = return_tokens.body();
        let visibility = self.visibility;
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });

        Ok(quote! {
            #(#argument_items)*
            #(#return_items)*
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #export_ident(#(#ffi_parameters),*) #return_type {
                #body
            }
        })
    }

    fn failure(&self) -> Result<TokenStream, Error> {
        match self
            .callable
            .params()
            .iter()
            .any(wrapper::param::requires_failure_return::<S>)
            || self.receiver.requires_failure_return()
        {
            true => <wrapper::returns::Failure as Render<S, _>>::render(
                wrapper::returns::Failure,
                wrapper::returns::FailureInput::new(
                    self.callable.returns(),
                    self.callable.error(),
                    self.expansion,
                ),
            ),
            false => Ok(TokenStream::new()),
        }
    }
}

impl RustCall {
    pub fn function(function: syn::Ident) -> Self {
        Self {
            owner: function.clone(),
            target: RustCallTarget::Function(function),
        }
    }

    pub fn associated(owner: TokenStream, method: syn::Ident) -> Self {
        Self {
            owner: method.clone(),
            target: RustCallTarget::Associated { owner, method },
        }
    }

    pub fn method(receiver: syn::Ident, method: syn::Ident) -> Self {
        Self {
            owner: method.clone(),
            target: RustCallTarget::Method { receiver, method },
        }
    }

    pub fn expression(&self, arguments: &[TokenStream]) -> TokenStream {
        match &self.target {
            RustCallTarget::Function(function) => quote! { #function(#(#arguments),*) },
            RustCallTarget::Associated { owner, method } => {
                quote! { #owner::#method(#(#arguments),*) }
            }
            RustCallTarget::Method { receiver, method } => {
                quote! { #receiver.#method(#(#arguments),*) }
            }
        }
    }
}

impl ReceiverTokens {
    pub fn none() -> Self {
        Self {
            ffi_parameters: Vec::new(),
            conversions: Vec::new(),
            writebacks: Vec::new(),
            requires_failure_return: false,
        }
    }

    pub fn new(
        ffi_parameters: Vec<TokenStream>,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
        requires_failure_return: bool,
    ) -> Self {
        Self {
            ffi_parameters,
            conversions,
            writebacks,
            requires_failure_return,
        }
    }

    fn requires_failure_return(&self) -> bool {
        self.requires_failure_return
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
}

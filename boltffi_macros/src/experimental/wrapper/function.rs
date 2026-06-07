use boltffi_ast::{FunctionDef, Visibility};
use boltffi_binding::{ExecutionDecl, FunctionDecl};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Path, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    target::Target,
    wrapper::{self, Render},
};

/// A function wrapper renderer for one target surface.
///
/// The renderer receives a paired source and binding declaration, then renders only the
/// generated extern wrapper. The original Rust function item remains owned by the caller.
pub struct Renderer<'context, 'a, S: Target> {
    pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
    expansion: &'context Expansion<'a, S>,
}

impl<'context, 'a, S> Renderer<'context, 'a, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer:
        Render<S, wrapper::arguments::Input<'context, 'a, S>, Output = wrapper::arguments::Tokens>,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'context, 'a, S>, Output = TokenStream>,
    wrapper::returns::Renderer:
        Render<S, wrapper::returns::Input<'context, 'a, S>, Output = wrapper::returns::Tokens>,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'context, 'a, S>, Output = TokenStream>,
{
    /// Creates a renderer for one paired function declaration.
    pub fn new(
        pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
        expansion: &'context Expansion<'a, S>,
    ) -> Self {
        Self { pair, expansion }
    }

    /// Renders the generated extern wrapper.
    pub fn render(self) -> Result<TokenStream, Error> {
        let function = self.pair.binding();
        let source = self.pair.source();
        let source_signature = rust_api::Callable::function(source);
        let function_ident = Self::function_ident(source)?;
        let visibility = Self::visibility(source)?;
        if matches!(
            function.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return <wrapper::async_call::Renderer as Render<S, _>>::render(
                wrapper::async_call::Renderer,
                wrapper::async_call::Input::new(
                    function,
                    source_signature,
                    function_ident,
                    visibility,
                    self.expansion,
                ),
            );
        }

        let cfg = S::cfg_attr();
        let failure = match function
            .callable()
            .params()
            .iter()
            .any(wrapper::param::requires_failure_return::<S>)
        {
            true => <wrapper::returns::Failure as Render<S, _>>::render(
                wrapper::returns::Failure,
                wrapper::returns::FailureInput::new(
                    function.callable().returns(),
                    function.callable().error(),
                    self.expansion,
                ),
            )?,
            false => TokenStream::new(),
        };
        let wrapper_arguments = <wrapper::arguments::SyncRenderer as Render<S, _>>::render(
            wrapper::arguments::SyncRenderer,
            wrapper::arguments::Input::new(
                function.callable(),
                source_signature,
                failure,
                self.expansion,
            ),
        )?;
        let export_ident = format_ident!("{}", function.symbol().name().as_str());
        let argument_ffi_parameters = wrapper_arguments.ffi_parameters();
        let conversions = wrapper_arguments.conversions();
        let writebacks = wrapper_arguments.writebacks();
        let rust_arguments = wrapper_arguments.rust_arguments();
        let return_tokens = <wrapper::returns::Renderer as Render<S, _>>::render(
            wrapper::returns::Renderer,
            wrapper::returns::Input::new(
                function.callable().returns(),
                function.callable().error(),
                source_signature.returns(),
                source_signature.returns().written_type()?,
                wrapper::returns::RustInvocation::new(
                    function_ident,
                    conversions.to_vec(),
                    writebacks.to_vec(),
                    rust_arguments.to_vec(),
                ),
                self.expansion,
            ),
        )?;
        let ffi_parameters = argument_ffi_parameters
            .iter()
            .chain(return_tokens.ffi_parameters().iter())
            .collect::<Vec<_>>();
        let return_type = return_tokens.return_type();
        let body = return_tokens.body();
        let argument_items = wrapper_arguments.items();
        let return_items = return_tokens.items();
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

    fn function_ident(source: &FunctionDef) -> Result<Ident, Error> {
        parse_str(source.name.spelling()).map_err(|_| {
            Error::SourceSyntaxMismatch("source function name is not a Rust identifier")
        })
    }

    fn visibility(source: &FunctionDef) -> Result<TokenStream, Error> {
        match &source.source.visibility {
            Visibility::Private => Ok(TokenStream::new()),
            Visibility::Public => Ok(quote! { pub }),
            Visibility::Restricted(path) => {
                let path = parse_str::<Path>(path).map_err(|_| {
                    Error::SourceSyntaxMismatch("source visibility path is not a Rust path")
                })?;
                Ok(quote! { pub(in #path) })
            }
        }
    }
}

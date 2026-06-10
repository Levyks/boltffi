use boltffi_ast::{FunctionDef, Visibility};
use boltffi_binding::{ExecutionDecl, FunctionDecl};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Path, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    target::Target,
    wrapper::{self, Render},
};

use super::export;

/// A function wrapper renderer for one target surface.
///
/// The renderer receives a paired source and binding declaration, then renders only the
/// generated extern wrapper. The original Rust function item remains owned by the caller.
pub struct Renderer<'expansion, 'lowered, S: Target> {
    pair: DeclarationPair<'lowered, FunctionDef, FunctionDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S> Renderer<'expansion, 'lowered, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
{
    /// Creates a renderer for one paired function declaration.
    pub fn new(
        pair: DeclarationPair<'lowered, FunctionDef, FunctionDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
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

        export::Renderer::new(
            function.symbol(),
            function.callable(),
            source_signature,
            export::RustCall::function(function_ident),
            export::ReceiverTokens::none(),
            visibility,
            self.expansion,
        )
        .render()
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

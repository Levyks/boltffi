use boltffi_ast::{FunctionDef, Visibility};
use boltffi_binding::{ExecutionDecl, FunctionDecl};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Path, parse_str};

use crate::experimental::{
    decl::DeclarationPair,
    error::Error,
    render::{self, Rule as RenderRule, callable::signature},
    target::Target,
};

/// A function wrapper renderer for one target surface.
///
/// The rule receives a paired source and binding declaration, then renders only the
/// generated extern wrapper. The original Rust function item remains owned by the caller.
pub struct Rule<'a, S: Target> {
    pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
}

impl<'a, S> Rule<'a, S>
where
    S: Target,
    render::callable::Rule:
        RenderRule<S, render::callable::Input<'a, S>, Output = render::callable::Tokens>,
    render::returns::Failure:
        RenderRule<S, render::returns::FailureInput<'a, S>, Output = TokenStream>,
    render::returns::Rule:
        RenderRule<S, render::returns::Input<'a, S>, Output = render::returns::Tokens>,
    render::asynchronous::Rule:
        RenderRule<S, render::asynchronous::Input<'a, S>, Output = TokenStream>,
{
    /// Creates a renderer for one paired function declaration.
    pub fn new(pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>) -> Self {
        Self { pair }
    }

    /// Renders the generated extern wrapper.
    pub fn render(self) -> Result<TokenStream, Error> {
        let function = self.pair.binding();
        let source = self.pair.source();
        let source_signature = signature::Callable::function(source);
        let function_ident = Self::function_ident(source)?;
        let visibility = Self::visibility(source)?;
        if matches!(
            function.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return <render::asynchronous::Rule as RenderRule<S, _>>::apply(
                render::asynchronous::Rule,
                render::asynchronous::Input::new(
                    function,
                    source_signature,
                    function_ident,
                    visibility,
                ),
            );
        }

        let cfg = S::cfg_attr();
        let failure = <render::returns::Failure as RenderRule<S, _>>::apply(
            render::returns::Failure,
            render::returns::FailureInput::new(
                function.callable().returns(),
                function.callable().error(),
            ),
        )?;
        let callable = <render::callable::Rule as RenderRule<S, _>>::apply(
            render::callable::Rule,
            render::callable::Input::new(function.callable(), source_signature, failure),
        )?;
        let export_ident = format_ident!("{}", function.symbol().name().as_str());
        let callable_ffi_parameters = callable.ffi_parameters();
        let conversions = callable.conversions();
        let writebacks = callable.writebacks();
        let arguments = callable.arguments();
        let return_tokens = <render::returns::Rule as RenderRule<S, _>>::apply(
            render::returns::Rule,
            render::returns::Input::new(
                function.callable().returns(),
                function.callable().error(),
                source_signature.returns(),
                source_signature.returns().written_type()?,
                render::returns::RustInvocation::new(
                    function_ident,
                    conversions.to_vec(),
                    writebacks.to_vec(),
                    arguments.to_vec(),
                ),
            ),
        )?;
        let ffi_parameters = callable_ffi_parameters
            .iter()
            .chain(return_tokens.ffi_parameters().iter())
            .collect::<Vec<_>>();
        let return_type = return_tokens.return_type();
        let body = return_tokens.body();
        let callable_items = callable.items();
        let return_items = return_tokens.items();
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });

        Ok(quote! {
            #(#callable_items)*
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

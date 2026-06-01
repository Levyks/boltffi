use boltffi_ast::FunctionDef;
use boltffi_binding::FunctionDecl;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, PatType, ReturnType, Type};

use crate::experimental::{
    decl::DeclarationPair,
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule<'a, S: Target> {
    pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
}

impl<'a, S> Rule<'a, S>
where
    S: Target,
    for<'syntax> render::callable::Rule:
        RenderRule<S, render::callable::Input<'a, 'syntax, S>, Output = render::callable::Tokens>,
    render::returns::Rule:
        RenderRule<S, render::returns::Input<'a, S>, Output = render::returns::Tokens>,
{
    pub fn new(pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>) -> Self {
        Self { pair }
    }

    pub fn render_with_function(self, syntax: ItemFn) -> Result<TokenStream, Error> {
        let export = self.render_export(&syntax)?;

        Ok(quote! {
            #syntax
            #export
        })
    }

    fn render_export(self, syntax: &ItemFn) -> Result<TokenStream, Error> {
        let cfg = S::cfg_attr();
        let function = self.pair.binding();
        let syntax_params = Self::syntax_params(syntax)?;
        let callable = <render::callable::Rule as RenderRule<S, _>>::apply(
            render::callable::Rule,
            render::callable::Input::new(function.callable(), &syntax_params),
        )?;
        let export_ident = format_ident!("{}", function.symbol().name().as_str());
        let function_ident = &syntax.sig.ident;
        let visibility = &syntax.vis;
        let ffi_parameters = callable.ffi_parameters();
        let conversions = callable.conversions();
        let arguments = callable.arguments();
        let return_tokens = <render::returns::Rule as RenderRule<S, _>>::apply(
            render::returns::Rule,
            render::returns::Input::new(
                function.callable().returns(),
                Self::syntax_return_type(syntax),
                render::returns::RustInvocation::new(
                    function_ident.clone(),
                    conversions.to_vec(),
                    arguments.to_vec(),
                ),
            ),
        )?;
        let return_type = return_tokens.return_type();
        let body = return_tokens.body();
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });

        Ok(quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #export_ident(#(#ffi_parameters),*) #return_type {
                #body
            }
        })
    }

    fn syntax_params(syntax: &ItemFn) -> Result<Vec<&PatType>, Error> {
        syntax
            .sig
            .inputs
            .iter()
            .map(|arg| match arg {
                FnArg::Typed(typed) => Ok(typed),
                FnArg::Receiver(_) => Err(Error::SourceSyntaxMismatch(
                    "function syntax unexpectedly contains a receiver",
                )),
            })
            .collect()
    }

    fn syntax_return_type(syntax: &ItemFn) -> Option<Type> {
        match &syntax.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(ty.as_ref().clone()),
        }
    }
}

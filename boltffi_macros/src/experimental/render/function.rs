use boltffi_ast::FunctionDef;
use boltffi_binding::FunctionDecl;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ItemFn;

use crate::experimental::{
    decl::DeclarationPair,
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule<'a, S: Target> {
    pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
}

impl<'a, S: Target> Rule<'a, S> {
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
        let callable = <render::callable::Rule as RenderRule<S, _>>::apply(
            render::callable::Rule,
            function.callable(),
        )?;
        let export_ident = format_ident!("{}", function.symbol().name().as_str());
        let function_ident = &syntax.sig.ident;
        let visibility = &syntax.vis;
        let ffi_params = callable.ffi_params();
        let call_args = callable.call_args();
        let return_type = callable.return_type();
        let safety = (!ffi_params.is_empty()).then(|| quote! { unsafe });

        Ok(quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #export_ident(#(#ffi_params),*) #return_type {
                #function_ident(#(#call_args),*)
            }
        })
    }
}

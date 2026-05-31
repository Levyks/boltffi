use boltffi_ast::FunctionDef;
use boltffi_binding::FunctionDecl;
use proc_macro2::TokenStream;
use syn::ItemFn;

use crate::experimental::{
    decl::{DeclarationPair, PairedDeclaration, SourceDeclaration},
    error::Error,
    render,
    syntax::Expand,
    target::Target,
};

pub struct ExpandableFunction {
    syntax: ItemFn,
}

impl ExpandableFunction {
    pub fn new(syntax: ItemFn) -> Self {
        Self { syntax }
    }
}

impl<'a, S: Target> Expand<'a, S> for ExpandableFunction {
    type Source = FunctionDef;
    type Binding = FunctionDecl<S>;

    fn source(source: &Self::Source) -> SourceDeclaration<'_> {
        SourceDeclaration::Function(source)
    }

    fn pair(
        paired: PairedDeclaration<'_, S>,
    ) -> Result<DeclarationPair<'_, Self::Source, Self::Binding>, Error> {
        match paired {
            PairedDeclaration::Function(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }

    fn render(
        self,
        pair: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
    ) -> Result<TokenStream, Error> {
        render::function::Rule::new(pair).render_with_function(self.syntax)
    }
}

use boltffi_ast::FunctionDef;
use boltffi_binding::{FunctionDecl, Surface};
use proc_macro2::TokenStream;
use syn::ItemFn;

use crate::experimental::{
    decl::{DeclarationPair, PairedDeclaration, SourceDeclaration},
    error::Error,
    render::{self, Rule as RenderRule},
    syntax::{ExpandableDeclaration, ItemRenderer, RenderableItem},
    target::Target,
};

/// The source and lowered declaration pair for a free function expansion.
///
/// The marker carries no syntax. The macro item remains owned by the caller, while this
/// type only selects `FunctionDef` from the source contract and `FunctionDecl<S>` from the
/// lowered bindings for a specific target surface.
pub struct ExpandableFunction;

impl ExpandableDeclaration for ExpandableFunction {
    type Source = FunctionDef;

    type Binding<'a, S: Surface> = DeclarationPair<'a, FunctionDef, FunctionDecl<S>>;

    fn source(source: &Self::Source) -> SourceDeclaration<'_> {
        SourceDeclaration::Function(source)
    }

    fn binding<'a, S: Surface>(
        paired: PairedDeclaration<'a, S>,
    ) -> Result<Self::Binding<'a, S>, Error> {
        match paired {
            PairedDeclaration::Function(pair) => Ok(pair),
            _ => Err(Error::WrongDeclaration),
        }
    }
}

impl RenderableItem for ExpandableFunction {
    type Syntax = ItemFn;

    type Renderer<'a, S: Target> = render::function::Rule<'a, S>;
}

impl<'a, S> ItemRenderer<'a, S, ExpandableFunction> for render::function::Rule<'a, S>
where
    S: Target,
    for<'params, 'syntax> render::callable::Rule: RenderRule<
            S,
            render::callable::Input<'a, 'params, 'syntax, S>,
            Output = render::callable::Tokens,
        >,
    render::returns::Failure:
        RenderRule<S, render::returns::FailureInput<'a, S>, Output = TokenStream>,
    render::returns::Rule:
        RenderRule<S, render::returns::Input<'a, S>, Output = render::returns::Tokens>,
    for<'syntax> render::asynchronous::Rule:
        RenderRule<S, render::asynchronous::Input<'a, 'syntax, S>, Output = TokenStream>,
{
    fn render(
        binding: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
        syntax: &ItemFn,
    ) -> Result<TokenStream, Error> {
        Self::new(binding).render(syntax)
    }
}

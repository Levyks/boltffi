use proc_macro2::TokenStream;

use super::decl::{DeclarationPair, PairedDeclaration, SourceDeclaration};
use super::error::Error;
use super::target::Target;

pub mod function;

pub trait Expand<'a, S: Target>: Sized {
    type Source;

    type Binding;

    fn source(source: &Self::Source) -> SourceDeclaration<'_>;

    fn pair(
        paired: PairedDeclaration<'_, S>,
    ) -> Result<DeclarationPair<'_, Self::Source, Self::Binding>, Error>;

    fn render(
        self,
        pair: DeclarationPair<'a, Self::Source, Self::Binding>,
    ) -> Result<TokenStream, Error>;
}

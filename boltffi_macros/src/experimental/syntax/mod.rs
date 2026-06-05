use boltffi_binding::Surface;
use proc_macro2::TokenStream;

use crate::experimental::{
    decl::{PairedDeclaration, SourceDeclaration},
    error::Error,
    expansion::Expansion,
    target::Target,
};

pub mod function;

/// A scanned declaration kind that has a lowered binding declaration.
///
/// Implementations define the exact source declaration and lowered binding pair for one
/// expandable Rust construct. The trait contains no syntax ownership and no rendering behavior;
/// it is only the typed key used by `Expansion<S>` when selecting a declaration pair.
///
/// # Example
///
/// ```rust,ignore
/// pub struct ExpandableFunction;
///
/// impl ExpandableDeclaration for ExpandableFunction {
///     type Source = FunctionDef;
///
///     type Binding<'a, S: Surface> = DeclarationPair<'a, FunctionDef, FunctionDecl<S>>;
///
///     fn source(source: &FunctionDef) -> SourceDeclaration<'_> {
///         SourceDeclaration::Function(source)
///     }
///
///     fn binding<'a, S: Surface>(
///         paired: PairedDeclaration<'a, S>,
///     ) -> Result<Self::Binding<'a, S>, Error> {
///         match paired {
///             PairedDeclaration::Function(pair) => Ok(pair),
///             _ => Err(Error::WrongDeclaration),
///         }
///     }
/// }
/// ```
pub trait ExpandableDeclaration {
    /// The scanned AST declaration selected by this expandable kind.
    type Source;

    /// The lowered declaration pair returned for target surface `S`.
    type Binding<'a, S: Surface>
    where
        Self: 'a;

    /// Returns the source declaration view used as the pairing key.
    fn source(source: &Self::Source) -> SourceDeclaration<'_>;

    /// Extracts this kind's typed binding pair from a generic paired declaration.
    fn binding<'a, S: Surface>(
        paired: PairedDeclaration<'a, S>,
    ) -> Result<Self::Binding<'a, S>, Error>;
}

/// A Rust syntax item that renders from a scanned declaration and its lowered binding.
///
/// Implementations bind three facts together: the scanned declaration type, the syntax type
/// received by the macro invocation, and the renderer selected for target surface `S`. Rendering
/// always starts by pairing through `Expansion<S>`, so item renderers cannot search declarations
/// by name or lower source again.
///
/// # Example
///
/// ```rust,ignore
/// impl RenderableItem for ExpandableFunction {
///     type Syntax = ItemFn;
///
///     type Renderer<'a, S: Target> = render::function::Rule<'a, S>;
/// }
///
/// let wrapper = ExpandableFunction::render(
///     &native_expansion,
///     source_function,
///     &item_fn,
/// )?;
/// ```
pub trait RenderableItem: ExpandableDeclaration + Sized {
    /// The Rust syntax node accepted by the renderer for this item kind.
    type Syntax;

    /// The renderer that emits wrapper tokens for target surface `S`.
    type Renderer<'a, S: Target>
    where
        Self: 'a;

    /// Renders the wrapper tokens for one syntax item and source declaration.
    ///
    /// The original Rust syntax item is not emitted here. Callers compose the original item and
    /// the returned wrapper tokens according to the target set they are expanding.
    fn render<'a, S>(
        expansion: &Expansion<'a, S>,
        source: &'a Self::Source,
        syntax: &Self::Syntax,
    ) -> Result<TokenStream, Error>
    where
        Self: 'a,
        S: Target,
        Self::Renderer<'a, S>: ItemRenderer<'a, S, Self>,
    {
        Self::Renderer::render(expansion.declaration::<Self>(source)?, syntax)
    }
}

/// A target renderer for one renderable Rust syntax item.
///
/// Implementations receive the typed declaration pair selected by `Expansion<S>` and the syntax
/// item held by the macro invocation. The output is only the generated wrapper tokens.
///
/// # Example
///
/// ```rust,ignore
/// impl<'a, S> ItemRenderer<'a, S, ExpandableFunction> for render::function::Rule<'a, S>
/// where
///     S: Target,
/// {
///     fn render(
///         binding: DeclarationPair<'a, FunctionDef, FunctionDecl<S>>,
///         syntax: &ItemFn,
///     ) -> Result<TokenStream, Error> {
///         Self::new(binding).render(syntax)
///     }
/// }
/// ```
pub trait ItemRenderer<'a, S, I>
where
    S: Target,
    I: RenderableItem + 'a,
{
    /// Renders wrapper tokens from the paired binding declaration and Rust syntax item.
    fn render(binding: I::Binding<'a, S>, syntax: &I::Syntax) -> Result<TokenStream, Error>;
}

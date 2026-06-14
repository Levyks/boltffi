use boltffi_ast::MethodDef;
use boltffi_binding::{
    ExecutionDecl, ExportedCallable, ExportedMethodDecl, InitializerDecl, NativeSymbol,
};
use proc_macro2::TokenStream;
use syn::{Ident, parse_str};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{self, Render, export},
};

pub trait Owner<'expansion, 'lowered, S: Target> {
    fn declarations(&self) -> rust_api::MethodDeclarations<'lowered>;

    fn source_callable(&self, method: &'lowered MethodDef) -> rust_api::Callable<'lowered>;

    fn receiver(
        &self,
        callable: &'lowered ExportedCallable<S>,
        method: Ident,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>;
}

pub struct Renderer<'expansion, 'lowered, S: Target, O> {
    owner: O,
    initializers: &'lowered [InitializerDecl<S>],
    methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Export<'lowered, S: Target> {
    kind: ExportKind,
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source_method: &'lowered MethodDef,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ExportKind {
    Initializer,
    Method,
}

impl<'expansion, 'lowered, S: Target, O> Renderer<'expansion, 'lowered, S, O> {
    pub fn new(
        owner: O,
        initializers: &'lowered [InitializerDecl<S>],
        methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            owner,
            initializers,
            methods,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        O: Owner<'expansion, 'lowered, S>,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        let declarations = self.owner.declarations();
        let initializers = self
            .initializers
            .iter()
            .map(|initializer| {
                let source_method = declarations.resolve(initializer.name())?;
                Export::initializer(initializer, source_method).render(&self.owner, self.expansion)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let methods = self
            .methods
            .iter()
            .map(|method| {
                let source_method = declarations.resolve(method.name())?;
                Export::method(method, source_method).render(&self.owner, self.expansion)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote::quote! {
            #(#initializers)*
            #(#methods)*
        })
    }
}

impl<'lowered, S: Target> Export<'lowered, S> {
    fn initializer(
        initializer: &'lowered InitializerDecl<S>,
        source_method: &'lowered MethodDef,
    ) -> Self {
        Self {
            kind: ExportKind::Initializer,
            symbol: initializer.symbol(),
            callable: initializer.callable(),
            source_method,
        }
    }

    fn method(
        method: &'lowered ExportedMethodDecl<S, NativeSymbol>,
        source_method: &'lowered MethodDef,
    ) -> Self {
        Self {
            kind: ExportKind::Method,
            symbol: method.target(),
            callable: method.callable(),
            source_method,
        }
    }

    fn render<'expansion, O>(
        self,
        owner: &O,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<TokenStream, Error>
    where
        O: Owner<'expansion, 'lowered, S>,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    {
        if self.kind == ExportKind::Initializer && self.callable.receiver().is_some() {
            return Err(Error::SourceSyntaxMismatch(
                "initializer binding unexpectedly has a receiver",
            ));
        }

        let method = method_ident(self.source_method)?;
        let (receiver, rust_call) = owner.receiver(self.callable, method, expansion)?;
        let source_callable = owner.source_callable(self.source_method);
        let visibility =
            rust_api::VisibilityTokens::new(&self.source_method.source.visibility).into_tokens()?;

        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            return <wrapper::async_call::Renderer as Render<S, _>>::render(
                wrapper::async_call::Renderer,
                wrapper::async_call::Input::exported(
                    self.symbol,
                    self.callable,
                    source_callable,
                    rust_call,
                    receiver,
                    visibility,
                    expansion,
                ),
            );
        }

        export::Renderer::new(
            self.symbol,
            self.callable,
            source_callable,
            rust_call,
            receiver,
            visibility,
            expansion,
        )
        .render()
    }
}

fn method_ident(source: &MethodDef) -> Result<Ident, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source method name is not a Rust identifier"))
}

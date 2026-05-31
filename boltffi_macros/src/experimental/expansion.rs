use boltffi_ast::{FunctionDef, SourceContract};
use boltffi_binding::{FunctionDecl, LoweredBindings, Surface};
use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemFn;

use super::error::Error;
use super::index::{DeclPair, ExpansionIndex, PairedDecl, SourceDecl};

pub struct Expansion<'a, S: Surface> {
    source: &'a SourceContract,
    lowered: &'a LoweredBindings<S>,
    index: ExpansionIndex,
}

impl<'a, S: Surface> Expansion<'a, S> {
    pub fn new(source: &'a SourceContract, lowered: &'a LoweredBindings<S>) -> Self {
        Self {
            source,
            lowered,
            index: ExpansionIndex::new(lowered.bindings()),
        }
    }

    pub fn source(&self) -> &'a SourceContract {
        self.source
    }

    pub fn bindings(&self) -> &'a boltffi_binding::Bindings<S> {
        self.lowered.bindings()
    }

    pub fn decl(&self, source: SourceDecl<'a>) -> Result<PairedDecl<'a, S>, Error> {
        self.index.decl(self.lowered, source)
    }

    pub fn function(&self, source: &'a FunctionDef, item: ItemFn) -> Result<TokenStream, Error> {
        let pair = match self.decl(SourceDecl::Function(source))? {
            PairedDecl::Function(pair) => pair,
            _ => return Err(Error::WrongDeclaration),
        };
        self.expand_function(pair, item)
    }

    fn expand_function(
        &self,
        pair: DeclPair<'a, FunctionDef, FunctionDecl<S>>,
        item: ItemFn,
    ) -> Result<TokenStream, Error> {
        pair.source();
        pair.binding();
        Ok(quote! {
            #item
        })
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, FunctionDef, FunctionId, PackageInfo, Primitive, ReturnDef, SourceContract,
        TypeExpr,
    };
    use boltffi_binding::{Native, lower_with_declarations};
    use quote::quote;

    use super::Expansion;

    fn source_contract() -> SourceContract {
        let mut function = FunctionDef::new(
            FunctionId::new("demo::answer"),
            CanonicalName::single("answer"),
        );
        function.returns = ReturnDef::Value(TypeExpr::Primitive(Primitive::U32));

        let mut source = SourceContract::new(PackageInfo::new("demo", None));
        source.functions.push(function);
        source
    }

    #[test]
    fn function_expansion_uses_exact_source_declaration() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Native>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let item = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens = expansion
            .function(&source.functions[0], item)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
            }
            .to_string()
        );
    }
}

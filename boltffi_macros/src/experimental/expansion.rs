use boltffi_ast::{FunctionDef, SourceContract};
use boltffi_binding::{LoweredBindings, Surface};
use proc_macro2::TokenStream;
use syn::ItemFn;

use super::decl::DeclarationPair;
use super::error::Error;
use super::index::ExpansionIndex;
use super::syntax::{self, Expand};
use super::target::Target;

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
}

impl<'a, S: Target> Expansion<'a, S> {
    pub fn pair<I>(
        &self,
        source: &'a I::Source,
    ) -> Result<DeclarationPair<'a, I::Source, I::Binding>, Error>
    where
        I: Expand<'a, S>,
    {
        I::pair(self.index.paired(self.lowered, I::source(source))?)
    }

    pub fn expand<I>(&self, source: &'a I::Source, syntax: I) -> Result<TokenStream, Error>
    where
        I: Expand<'a, S> + 'a,
    {
        let pair = self.pair::<I>(source)?;
        syntax.render(pair)
    }

    pub fn function(&self, source: &'a FunctionDef, syntax: ItemFn) -> Result<TokenStream, Error> {
        self.expand(source, syntax::function::ExpandableFunction::new(syntax))
    }
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName, FunctionDef, FunctionId, PackageInfo, Primitive, ReturnDef, SourceContract,
        TypeExpr,
    };
    use boltffi_binding::{Native, Wasm32, lower_with_declarations};
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
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(not(target_arch = "wasm32"))]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }

    #[test]
    fn wasm_function_expansion_uses_wasm_cfg() {
        let source = source_contract();
        let lowered = lower_with_declarations::<Wasm32>(&source).expect("lowered bindings");
        let expansion = Expansion::new(&source, &lowered);
        let syntax = syn::parse_quote! {
            pub fn answer() -> u32 {
                42
            }
        };

        let tokens = expansion
            .function(&source.functions[0], syntax)
            .expect("expanded function");

        assert_eq!(
            tokens.to_string(),
            quote! {
                pub fn answer() -> u32 {
                    42
                }
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub extern "C" fn boltffi_function_demo_answer() -> u32 {
                    answer()
                }
            }
            .to_string()
        );
    }
}

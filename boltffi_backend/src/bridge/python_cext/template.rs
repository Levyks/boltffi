use askama::Template as AskamaTemplate;

use crate::{
    bridge::{
        c::{Identifier, Literal, Statement, syntax::FunctionSyntax},
        python_cext::{LoadedFunction, PythonCExtBridgeContract},
    },
    core::Result,
};

#[derive(AskamaTemplate)]
#[template(path = "bridge/python_cext/loader.c", escape = "none")]
struct LoaderTemplate {
    c_header: Literal,
    loader_function: Identifier,
    free_function: Identifier,
    functions: Vec<FunctionView>,
}

struct FunctionView {
    symbol: Literal,
    typedef_name: Identifier,
    typedef_declaration: Statement,
    storage_name: Identifier,
}

pub struct Loader<'contract> {
    contract: &'contract PythonCExtBridgeContract,
}

impl<'contract> Loader<'contract> {
    pub fn new(contract: &'contract PythonCExtBridgeContract) -> Self {
        Self { contract }
    }

    pub fn render(self) -> Result<String> {
        Ok(LoaderTemplate {
            c_header: Literal::string(self.contract.c_header().as_str()),
            loader_function: self.contract.loader_method().c_function().clone(),
            free_function: self.contract.symbols().free_function().clone(),
            functions: self
                .contract
                .functions()
                .iter()
                .map(FunctionView::from_function)
                .collect::<Result<Vec<_>>>()?,
        }
        .render()?)
    }
}

impl FunctionView {
    fn from_function(function: &LoadedFunction) -> Result<Self> {
        Ok(Self {
            symbol: Literal::string(function.function().name()),
            typedef_name: function.typedef_name().clone(),
            typedef_declaration: FunctionSyntax::new(function.function())
                .pointer_typedef(function.typedef_name().as_str())?,
            storage_name: function.storage_name().clone(),
        })
    }
}

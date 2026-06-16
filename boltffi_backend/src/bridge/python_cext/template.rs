use askama::Template as AskamaTemplate;

use crate::{
    bridge::{
        c::syntax::FunctionSyntax,
        python_cext::{LoadedFunction, PythonCExtBridgeContract},
    },
    core::Result,
};

#[derive(AskamaTemplate)]
#[template(path = "bridge/python_cext/loader.c", escape = "none")]
struct LoaderTemplate {
    c_header: String,
    loader_function: String,
    free_function: String,
    functions: Vec<FunctionView>,
}

struct FunctionView {
    symbol: String,
    typedef_name: String,
    typedef_declaration: String,
    storage_name: String,
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
            c_header: self.contract.c_header().as_str().to_owned(),
            loader_function: self.contract.loader_method().c_function().to_owned(),
            free_function: self.contract.symbols().free_function().to_owned(),
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
            symbol: function.function().name().to_owned(),
            typedef_name: function.typedef_name().to_owned(),
            typedef_declaration: FunctionSyntax::new(function.function())
                .pointer_typedef(function.typedef_name())?,
            storage_name: function.storage_name().to_owned(),
        })
    }
}

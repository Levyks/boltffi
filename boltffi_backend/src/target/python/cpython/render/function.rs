use askama::Template as AskamaTemplate;
use boltffi_binding::{ErrorDecl, ExecutionDecl, FunctionDecl, Native};

use crate::{
    bridge::python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    core::{Emitted, Error, Result},
    target::python::{
        cpython::render::{argument, primitive, result},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/function.c", escape = "none")]
struct WrapperTemplate {
    python_name: String,
    wrapper: String,
    storage: String,
    params: Vec<argument::Conversion>,
    call_args: Vec<String>,
    returns: result::Conversion,
}

pub struct Wrapper {
    pub python_name: String,
    pub wrapper: String,
    pub storage: String,
    pub params: Vec<argument::Conversion>,
    pub call_args: Vec<String>,
    pub returns: result::Conversion,
    method: ExtensionMethod,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &FunctionDecl<Native>,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        if matches!(
            declaration.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async function",
            });
        }
        if !matches!(declaration.callable().error(), ErrorDecl::None(_)) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible function",
            });
        }
        let loaded =
            bridge
                .loaded_function(declaration.symbol())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "function without C bridge symbol",
                })?;
        let python_name = Name::new(declaration.name()).function();
        let wrapper = format!(
            "boltffi_python_callable_wrapper_{}",
            declaration.symbol().name().as_str()
        );
        let method =
            ExtensionMethod::new(python_name.clone(), wrapper.clone(), MethodFlags::FastCall)?;
        let params = declaration
            .callable()
            .params()
            .iter()
            .enumerate()
            .map(|(index, parameter)| argument::Conversion::from_parameter(index, parameter))
            .collect::<Result<Vec<_>>>()?;
        let call_args = params
            .iter()
            .flat_map(argument::Conversion::call_args)
            .collect();
        let returns = result::Conversion::from_plan(declaration.callable().returns().plan())?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().to_owned(),
            params,
            call_args,
            returns,
            method,
        })
    }

    pub fn method(&self) -> &ExtensionMethod {
        &self.method
    }

    pub fn render(self) -> Result<Emitted> {
        let source = WrapperTemplate {
            python_name: self.python_name,
            wrapper: self.wrapper,
            storage: self.storage,
            params: self.params,
            call_args: self.call_args,
            returns: self.returns,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        let params = self
            .params
            .iter()
            .filter_map(argument::Conversion::primitive)
            .collect::<Vec<_>>();
        params.into_iter().chain(self.returns.primitive()).collect()
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.returns.owned_buffer()
    }

    pub fn has_string_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_string)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_bytes)
    }
}

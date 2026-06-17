use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ErrorDecl, ExecutionDecl, ExportedCallable, FunctionDecl, Native, NativeSymbol,
};

use crate::{
    bridge::python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{argument, direct_vector, primitive, result},
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
    pub fn supports(callable: &ExportedCallable<Native>) -> bool {
        matches!(callable.execution(), ExecutionDecl::Synchronous(_))
            && matches!(callable.error(), ErrorDecl::None(_))
            && callable.params().iter().all(argument::Conversion::supports)
            && result::Conversion::supports(callable.returns().plan())
    }

    pub fn from_declaration(
        declaration: &FunctionDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_export(
            Name::new(declaration.name()).function(),
            declaration.symbol(),
            declaration.callable(),
            Vec::new(),
            bridge,
            context,
        )
    }

    pub fn from_export(
        python_name: String,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => {}
            ExecutionDecl::Asynchronous(_) => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "async function",
                });
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown function execution",
                });
            }
        }
        if !matches!(callable.error(), ErrorDecl::None(_)) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible function",
            });
        }
        let loaded = bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "function without C bridge symbol",
            })?;
        let wrapper = format!("boltffi_python_callable_wrapper_{}", symbol.name().as_str());
        let method =
            ExtensionMethod::new(python_name.clone(), wrapper.clone(), MethodFlags::FastCall)?;
        let value_args = callable
            .params()
            .iter()
            .enumerate()
            .map(|(offset, parameter)| {
                let index = receiver_args.len() + offset;
                argument::Conversion::from_parameter(index, parameter, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let params = receiver_args
            .into_iter()
            .chain(value_args)
            .collect::<Vec<_>>();
        let call_args = params
            .iter()
            .flat_map(argument::Conversion::call_args)
            .collect();
        let returns = result::Conversion::from_plan(callable.returns().plan(), bridge, context)?;
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

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::wire_primitive)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::direct_vector_element)
            .chain(self.returns.direct_vector_element())
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

    pub fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_raw_wire)
    }
}

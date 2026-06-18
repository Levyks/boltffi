use boltffi_binding::{ExecutionDecl, ExportedCallable, Native, native};

use crate::{
    core::{Error, Result},
    target::python::cpython::render::function::NativeFutureMethods,
};

use super::return_value::ReturnedValue;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableBody {
    asynchronous: bool,
    lines: Vec<String>,
}

impl CallableBody {
    pub fn from_callable(
        callable: &ExportedCallable<Native>,
        native_name: &str,
        native_call: String,
        returned: &ReturnedValue,
    ) -> Result<Self> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => Ok(Self::sync(returned.statement(native_call))),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle { .. }) => {
                Ok(Self::native_future(native_name, native_call, returned))
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async callable",
            }),
        }
    }
}

impl CallableBody {
    pub fn is_async(&self) -> bool {
        self.asynchronous
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.asynchronous
    }

    pub fn into_lines(self) -> Vec<String> {
        self.lines
    }

    fn sync(line: String) -> Self {
        Self {
            asynchronous: false,
            lines: vec![line],
        }
    }

    fn native_future(native_name: &str, native_call: String, returned: &ReturnedValue) -> Self {
        let methods = NativeFutureMethods::new(native_name);
        let future = "__boltffi_future";
        Self {
            asynchronous: true,
            lines: [
                format!("{future} = _BoltFfiNativeFuture("),
                format!("    handle={native_call},"),
                format!("    poll=_native.{},", methods.poll()),
                format!("    complete=_native.{},", methods.complete()),
                format!("    cancel=_native.{},", methods.cancel()),
                format!("    free=_native.{},", methods.free()),
                format!("    panic_message=_native.{},", methods.panic_message()),
                ")".to_owned(),
            ]
            .into_iter()
            .chain(returned.awaited_statement(format!("{future}.wait()")))
            .collect(),
        }
    }
}

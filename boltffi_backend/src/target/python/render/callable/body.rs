use boltffi_binding::{ExecutionDecl, ExportedCallable, Native, native};

use crate::{
    core::{Error, Result},
    target::python::syntax::{CallExpression, Expression, Identifier, Statement},
};

use super::{future::NativeFutureMethods, return_value::ReturnedValue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallableBody {
    asynchronous: bool,
    lines: Vec<Statement>,
}

impl CallableBody {
    pub fn from_callable(
        callable: &ExportedCallable<Native>,
        native_name: &Identifier,
        native_call: Expression,
        returned: &ReturnedValue,
    ) -> Result<Self> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => returned.statement(native_call).map(Self::sync),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle { .. }) => {
                Self::native_future(native_name, native_call, returned)
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

    pub fn into_lines(self) -> Vec<Statement> {
        self.lines
    }

    fn sync(line: Statement) -> Self {
        Self {
            asynchronous: false,
            lines: vec![line],
        }
    }

    fn native_future(
        native_name: &Identifier,
        native_call: Expression,
        returned: &ReturnedValue,
    ) -> Result<Self> {
        let methods = NativeFutureMethods::new(native_name.clone())?;
        let future = Identifier::parse("__boltffi_future")?;
        let native_module = Expression::identifier(Identifier::parse("_native")?);
        let constructor = Expression::identifier(Identifier::parse("_BoltFfiNativeFuture")?);
        let future_call = CallExpression::new(constructor)
            .keyword(Identifier::parse("handle")?, native_call)
            .keyword(
                Identifier::parse("poll")?,
                Expression::attribute(native_module.clone(), methods.poll().clone()),
            )
            .keyword(
                Identifier::parse("complete")?,
                Expression::attribute(native_module.clone(), methods.complete().clone()),
            )
            .keyword(
                Identifier::parse("cancel")?,
                Expression::attribute(native_module.clone(), methods.cancel().clone()),
            )
            .keyword(
                Identifier::parse("free")?,
                Expression::attribute(native_module.clone(), methods.free().clone()),
            )
            .keyword(
                Identifier::parse("panic_message")?,
                Expression::attribute(native_module, methods.panic_message().clone()),
            );
        let wait_call = Expression::call(CallExpression::new(Expression::attribute(
            Expression::identifier(future.clone()),
            Identifier::parse("wait")?,
        )));
        Ok(Self {
            asynchronous: true,
            lines: Statement::assign_call(future, future_call)
                .into_iter()
                .chain(returned.awaited_statement(wait_call)?)
                .collect(),
        })
    }
}

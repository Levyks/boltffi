use boltffi_binding::{
    ExportedMethodDecl, InitializerDecl, IntoRust, Native, NativeSymbol, ParamDecl,
};

use crate::{
    core::Result,
    target::python::{cpython::render::class as class_render, name_style::Name},
};

use super::{
    super::Package,
    body::CallableBody,
    parameter::ParameterStub,
    return_value::{ReturnStub, ReturnedValue},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssociatedCallable {
    pub receiver: bool,
    pub python_name: String,
    pub native_name: String,
    pub parameters: Vec<ParameterStub>,
    pub arguments: String,
    pub return_annotation: String,
    pub asynchronous: bool,
    pub body: Vec<String>,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
}

impl AssociatedCallable {
    pub fn from_class_initializer(
        initializer: &InitializerDecl<Native>,
        symbols: &class_render::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = initializer
            .callable()
            .params()
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect::<Result<Vec<_>>>()?;
        let arguments = Self::arguments(None, &parameters);
        let native_name = symbols.initializer(initializer.name());
        let native_call = format!("_native.{native_name}({arguments})");
        let returned = ReturnedValue::class_handle(symbols.class_name());
        let body = CallableBody::from_callable(
            initializer.callable(),
            &native_name,
            native_call,
            &returned,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers);
        Ok(Self {
            receiver: false,
            python_name: Name::new(initializer.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            return_annotation: symbols.class_name().to_owned(),
            parameters,
            uses_wire_helpers,
        })
    }

    pub fn from_class_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        symbols: &class_render::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let receiver = method.callable().receiver().is_some();
        let parameters = method
            .callable()
            .params()
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect::<Result<Vec<_>>>()?;
        let returned = ReturnStub::from_callable(method.callable(), package)?;
        let arguments = Self::arguments(receiver.then_some("self._handle"), &parameters);
        let native_name = symbols.method(method.name());
        let native_call = format!("_native.{native_name}({arguments})");
        let body = CallableBody::from_callable(
            method.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            receiver,
            python_name: Name::new(method.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            parameters,
            return_annotation: returned.into_annotation(),
            uses_wire_helpers,
        })
    }

    pub fn from_value_initializer(
        initializer: &InitializerDecl<Native>,
        native_name: String,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = Self::parameters(initializer.callable().params(), package)?;
        let returned = ReturnStub::from_callable(initializer.callable(), package)?;
        let arguments = Self::arguments(None, &parameters);
        let native_call = format!("_native.{native_name}({arguments})");
        let body = CallableBody::from_callable(
            initializer.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            receiver: false,
            python_name: Name::new(initializer.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            return_annotation: returned.into_annotation(),
            parameters,
            uses_wire_helpers,
        })
    }

    pub fn from_value_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        native_name: String,
        receiver: Option<&str>,
        mutated_receiver_type: Option<&str>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = Self::parameters(method.callable().params(), package)?;
        let returned = match mutated_receiver_type {
            Some(annotation) => ReturnStub::native(annotation),
            None => ReturnStub::from_callable(method.callable(), package)?,
        };
        let arguments = Self::arguments(receiver, &parameters);
        let native_call = format!("_native.{native_name}({arguments})");
        let body = CallableBody::from_callable(
            method.callable(),
            &native_name,
            native_call,
            returned.returned_value(),
        )?;
        let uses_wire_helpers =
            parameters.iter().any(ParameterStub::uses_wire_helpers) || returned.uses_wire_helpers();
        Ok(Self {
            receiver: receiver.is_some(),
            python_name: Name::new(method.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            parameters,
            return_annotation: returned.into_annotation(),
            uses_wire_helpers,
        })
    }
}

impl AssociatedCallable {
    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn uses_async_helpers(&self) -> bool {
        self.uses_async_helpers
    }

    pub fn is_async(&self) -> bool {
        self.asynchronous
    }

    pub fn uses_sequence_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_sequence_annotation)
    }

    pub fn uses_callable_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_callable_annotation)
    }

    pub fn validate_names(&self, owner: &str) -> Result<()> {
        ParameterStub::scope(
            format!("method `{}.{}`", owner, self.python_name),
            &self.parameters,
        )
        .map(|_| ())
    }

    pub fn member_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("method `{}`", self.python_name),
        )
    }

    fn arguments(receiver: Option<&str>, parameters: &[ParameterStub]) -> String {
        receiver
            .into_iter()
            .map(str::to_owned)
            .chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.argument.clone()),
            )
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn parameters(
        parameters: &[ParamDecl<Native, IntoRust>],
        package: &Package<'_, '_>,
    ) -> Result<Vec<ParameterStub>> {
        parameters
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect()
    }
}

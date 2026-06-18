use boltffi_binding::{
    CanonicalName, HandlePresence, HandleTarget, IncomingParam, IntoRust, Native, ParamDecl,
    ParamPlan, ParamPlanRender, Primitive, Receive, TypeRef, WritePlan, native,
};

use crate::{
    core::Result,
    target::python::{codec::Expression as CodecExpression, name_style::Name},
};

use super::super::{NameScope, Package, type_hint::TypeHint};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterStub {
    pub name: String,
    pub annotation: String,
    pub argument: String,
    uses_sequence_annotation: bool,
    uses_callable_annotation: bool,
    uses_wire_helpers: bool,
}

impl ParameterStub {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let name = Name::new(parameter.name()).function();
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Ok(Self {
                name: name.clone(),
                annotation: "Callable[..., object]".to_owned(),
                argument: name,
                uses_sequence_annotation: false,
                uses_callable_annotation: true,
                uses_wire_helpers: false,
            });
        };
        let argument = Self::argument(plan, parameter.name(), package)?;
        let uses_wire_helpers = Self::uses_wire(plan, package)?;
        let annotation = TypeHint::from_parameter(plan, package)?;
        Ok(Self {
            name,
            uses_sequence_annotation: annotation.uses_sequence(),
            uses_callable_annotation: false,
            annotation: annotation.into_string(),
            argument,
            uses_wire_helpers,
        })
    }
}

impl ParameterStub {
    pub fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    pub fn uses_sequence_annotation(&self) -> bool {
        self.uses_sequence_annotation
    }

    pub fn uses_callable_annotation(&self) -> bool {
        self.uses_callable_annotation
    }

    pub fn parameter_name(&self) -> (String, String) {
        (self.name.clone(), format!("parameter `{}`", self.name))
    }

    pub fn scope(label: impl Into<String>, parameters: &[Self]) -> Result<NameScope> {
        NameScope::new(label).insert_all(parameters.iter().map(Self::parameter_name))
    }

    fn argument(
        plan: &ParamPlan<Native, IntoRust>,
        name: &CanonicalName,
        package: &Package<'_, '_>,
    ) -> Result<String> {
        plan.render_with(&mut StubArgument {
            name: Name::new(name).function(),
            package,
        })
    }

    fn uses_wire(plan: &ParamPlan<Native, IntoRust>, package: &Package<'_, '_>) -> Result<bool> {
        plan.render_with(&mut WireHelperUse { package })
    }
}

struct StubArgument<'package, 'binding, 'bridge> {
    name: String,
    package: &'package Package<'binding, 'bridge>,
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for StubArgument<'_, '_, '_> {
    type Output = Result<String>;

    fn direct(&mut self, _: &TypeRef, _: Receive) -> Self::Output {
        Ok(self.name.clone())
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        codec: &WritePlan,
        _: native::BufferShape,
        _: Receive,
    ) -> Self::Output {
        CodecExpression::write_argument(codec, self.package).map(CodecExpression::into_string)
    }

    fn handle(
        &mut self,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        match (target, presence) {
            (HandleTarget::Class(_), HandlePresence::Required) => {
                Ok(format!("{}._handle", self.name))
            }
            (HandleTarget::Class(_), HandlePresence::Nullable) => Ok(format!(
                "(0 if {} is None else {}._handle)",
                self.name, self.name
            )),
            _ => Ok(self.name.clone()),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(self.name.clone())
    }

    fn direct_vector(&mut self, _: &TypeRef) -> Self::Output {
        Ok(self.name.clone())
    }
}

struct WireHelperUse<'package, 'binding, 'bridge> {
    package: &'package Package<'binding, 'bridge>,
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for WireHelperUse<'_, '_, '_> {
    type Output = Result<bool>;

    fn direct(&mut self, _: &TypeRef, _: Receive) -> Self::Output {
        Ok(false)
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        codec: &WritePlan,
        shape: native::BufferShape,
        _: Receive,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => {
                CodecExpression::write_argument(codec, self.package).map(|_| true)
            }
            _ => Ok(false),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        Ok(false)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(false)
    }

    fn direct_vector(&mut self, _: &TypeRef) -> Self::Output {
        Ok(false)
    }
}

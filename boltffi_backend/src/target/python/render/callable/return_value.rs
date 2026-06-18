use std::marker::PhantomData;

use boltffi_binding::{
    ClosureReturn, ErrorDecl, ExportedCallable, HandlePresence, HandleTarget, Native, OutOfRust,
    Primitive, ReadPlan, ReturnPlan, ReturnPlanRender, ReturnValueSlot, TypeRef, native,
};

use crate::{
    core::{Error, Result},
    target::python::codec::Expression as CodecExpression,
};

use super::super::{Package, type_hint::TypeHint};

pub struct ReturnStub {
    annotation: String,
    value: ReturnedValue,
}

impl ReturnStub {
    pub fn native(annotation: impl Into<String>) -> Self {
        Self {
            annotation: annotation.into(),
            value: ReturnedValue::Native,
        }
    }

    pub fn from_callable(
        callable: &ExportedCallable<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match callable.error() {
            ErrorDecl::None(_) => Self::from_plan(callable.returns().plan(), package),
            ErrorDecl::EncodedViaReturnSlot {
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_success_plan(callable.returns().plan(), package),
            ErrorDecl::EncodedViaReturnSlot { .. } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error buffer shape",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible callable stub",
            }),
        }
    }

    pub fn from_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        Ok(Self {
            annotation: TypeHint::from_return(plan, package)?.into_string(),
            value: ReturnedValue::from_plan(plan, package)?,
        })
    }
}

impl ReturnStub {
    pub fn annotation(&self) -> &str {
        &self.annotation
    }

    pub fn expression(&self, native_call: String) -> String {
        self.value.expression(native_call)
    }

    pub fn into_annotation(self) -> String {
        self.annotation
    }

    pub fn returned_value(&self) -> &ReturnedValue {
        &self.value
    }

    pub fn uses_wire_helpers(&self) -> bool {
        self.value.uses_wire_helpers()
    }

    fn from_success_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        Ok(Self {
            annotation: TypeHint::from_return(plan, package)?.into_string(),
            value: ReturnedValue::from_success_plan(plan, package)?,
        })
    }
}

pub enum ReturnedValue {
    Void,
    Native,
    ClassHandle(String),
    Wire(String),
}

impl ReturnedValue {
    pub fn class_handle(class_name: impl Into<String>) -> Self {
        Self::ClassHandle(class_name.into())
    }

    pub fn from_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        plan.render_with(&mut ReturnedValueRender::<CallableReturn>::new(package))
    }

    pub fn from_success_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        plan.render_with(&mut ReturnedValueRender::<FallibleSuccessReturn>::new(
            package,
        ))
    }

    pub fn statement(&self, native_call: String) -> String {
        match self {
            Self::Void => native_call,
            Self::Native | Self::ClassHandle(_) | Self::Wire(_) => {
                format!("return {}", self.expression(native_call))
            }
        }
    }

    pub fn expression(&self, native_call: String) -> String {
        match self {
            Self::Void => native_call,
            Self::Native => native_call,
            Self::ClassHandle(class_name) => {
                format!("{class_name}._from_handle({native_call})")
            }
            Self::Wire(decode) => {
                format!("_boltffi_read_wire({native_call}, lambda reader: {decode})")
            }
        }
    }

    pub fn uses_wire_helpers(&self) -> bool {
        matches!(self, Self::Wire(_))
    }

    pub fn awaited_statement(&self, wait_call: String) -> Vec<String> {
        let value = "__boltffi_value";
        match self {
            Self::Void => vec![format!("await {wait_call}")],
            Self::Native => vec![format!("return await {wait_call}")],
            Self::ClassHandle(class_name) => vec![
                format!("{value} = await {wait_call}"),
                format!("return {class_name}._from_handle({value})"),
            ],
            Self::Wire(decode) => vec![
                format!("{value} = await {wait_call}"),
                format!("return _boltffi_read_wire({value}, lambda reader: {decode})"),
            ],
        }
    }

    fn from_encoded_plan(codec: &ReadPlan, package: &Package<'_, '_>) -> Result<Self> {
        CodecExpression::read_return(codec, package).map(|decode| Self::Wire(decode.into_string()))
    }
}

struct CallableReturn;

struct FallibleSuccessReturn;

trait ReturnDelivery {
    fn slot(slot: ReturnValueSlot) -> Result<()>;

    fn native() -> Result<ReturnedValue>;

    fn unsupported_shape() -> &'static str;
}

impl ReturnDelivery for CallableReturn {
    fn slot(_: ReturnValueSlot) -> Result<()> {
        Ok(())
    }

    fn native() -> Result<ReturnedValue> {
        Ok(ReturnedValue::Native)
    }

    fn unsupported_shape() -> &'static str {
        "unknown return stub"
    }
}

impl ReturnDelivery for FallibleSuccessReturn {
    fn slot(slot: ReturnValueSlot) -> Result<()> {
        match slot {
            ReturnValueSlot::OutPointer => Ok(()),
            ReturnValueSlot::ReturnSlot => Err(Error::UnsupportedTarget {
                target: "python",
                shape: Self::unsupported_shape(),
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown return stub",
            }),
        }
    }

    fn native() -> Result<ReturnedValue> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: Self::unsupported_shape(),
        })
    }

    fn unsupported_shape() -> &'static str {
        "fallible success stub"
    }
}

struct ReturnedValueRender<'package, 'binding, 'bridge, D> {
    package: &'package Package<'binding, 'bridge>,
    delivery: PhantomData<D>,
}

impl<'package, 'binding, 'bridge, D> ReturnedValueRender<'package, 'binding, 'bridge, D> {
    fn new(package: &'package Package<'binding, 'bridge>) -> Self {
        Self {
            package,
            delivery: PhantomData,
        }
    }
}

impl<'plan, D> ReturnPlanRender<'plan, Native, OutOfRust> for ReturnedValueRender<'_, '_, '_, D>
where
    D: ReturnDelivery,
{
    type Output = Result<ReturnedValue>;

    fn void(&mut self) -> Self::Output {
        Ok(ReturnedValue::Void)
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &TypeRef) -> Self::Output {
        D::slot(slot).and_then(|()| D::native())
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &TypeRef,
        codec: &ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        D::slot(slot)?;
        match shape {
            native::BufferShape::Buffer => ReturnedValue::from_encoded_plan(codec, self.package),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: D::unsupported_shape(),
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        D::slot(slot)?;
        match (target, presence) {
            (HandleTarget::Class(class_id), HandlePresence::Required) => Ok(
                ReturnedValue::ClassHandle(self.package.class_name(class_id)?),
            ),
            _ => D::native(),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        D::native()
    }

    fn direct_vector(&mut self, _: &TypeRef) -> Self::Output {
        D::native()
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: D::unsupported_shape(),
        })
    }
}

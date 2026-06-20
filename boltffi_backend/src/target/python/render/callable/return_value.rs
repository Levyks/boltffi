use std::marker::PhantomData;

use boltffi_binding::{
    ClosureReturn, DirectValueType, DirectVectorElementType, ErrorDecl, ExportedCallable,
    HandlePresence, HandleTarget, Native, OutOfRust, Primitive, ReadPlan, ReturnPlan,
    ReturnPlanRender, ReturnValueSlot, TypeRef, native,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::Expression as CodecExpression,
        syntax::{CallExpression, Expression, Identifier, Statement, TypeAnnotation},
    },
};

use super::super::{Package, type_hint::TypeHint};

pub struct ReturnStub {
    annotation: TypeAnnotation,
    value: ReturnedValue,
}

impl ReturnStub {
    pub fn native(annotation: TypeAnnotation) -> Self {
        Self {
            annotation,
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
            annotation: TypeHint::from_return(plan, package)?.into_annotation(),
            value: ReturnedValue::from_plan(plan, package)?,
        })
    }
}

impl ReturnStub {
    pub fn expression(&self, native_call: Expression) -> Result<Expression> {
        self.value.expression(native_call)
    }

    pub fn into_annotation(self) -> TypeAnnotation {
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
            annotation: TypeHint::from_return(plan, package)?.into_annotation(),
            value: ReturnedValue::from_success_plan(plan, package)?,
        })
    }
}

pub enum ReturnedValue {
    Void,
    Native,
    ClassHandle(Identifier),
    Wire(Expression),
}

impl ReturnedValue {
    pub fn class_handle(class_name: Identifier) -> Self {
        Self::ClassHandle(class_name)
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

    pub fn statement(&self, native_call: Expression) -> Result<Statement> {
        match self {
            Self::Void => Ok(Statement::expression(native_call)),
            Self::Native | Self::ClassHandle(_) | Self::Wire(_) => {
                self.expression(native_call).map(Statement::return_value)
            }
        }
    }

    pub fn expression(&self, native_call: Expression) -> Result<Expression> {
        match self {
            Self::Void | Self::Native => Ok(native_call),
            Self::ClassHandle(class_name) => Ok(Expression::call(
                CallExpression::new(Expression::attribute(
                    Expression::identifier(class_name.clone()),
                    Identifier::parse("_from_handle")?,
                ))
                .positional(native_call),
            )),
            Self::Wire(decode) => Ok(Expression::call(
                CallExpression::new(Expression::identifier(Identifier::parse(
                    "_boltffi_read_wire",
                )?))
                .positional(native_call)
                .positional(Expression::lambda(
                    Identifier::parse("reader")?,
                    decode.clone(),
                )),
            )),
        }
    }

    pub fn uses_wire_helpers(&self) -> bool {
        matches!(self, Self::Wire(_))
    }

    pub fn awaited_statement(&self, wait_call: Expression) -> Result<Vec<Statement>> {
        let value = Identifier::parse("__boltffi_value")?;
        let awaited = Expression::await_value(wait_call);
        match self {
            Self::Void => Ok(vec![Statement::expression(awaited)]),
            Self::Native => Ok(vec![Statement::return_value(awaited)]),
            Self::ClassHandle(_) | Self::Wire(_) => Ok(vec![
                Statement::assign(value.clone(), awaited),
                Statement::return_value(self.expression(Expression::identifier(value))?),
            ]),
        }
    }

    fn from_encoded_plan(codec: &ReadPlan, package: &Package<'_, '_>) -> Result<Self> {
        CodecExpression::read_return(codec, package)
            .map(|decode| Self::Wire(decode.into_expression()))
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
        Ok(ReturnedValue::Native)
    }

    fn unsupported_shape() -> &'static str {
        "fallible success return"
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

    fn direct(&mut self, slot: ReturnValueSlot, _: &DirectValueType) -> Self::Output {
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
                ReturnedValue::class_handle(self.package.class_name(class_id)?),
            ),
            _ => D::native(),
        }
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        D::native()
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType) -> Self::Output {
        D::native()
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: D::unsupported_shape(),
        })
    }
}

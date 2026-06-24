use askama::Template as AskamaTemplate;
use boltffi_binding::{
    DirectValueType, Direction, ExecutionDecl, FunctionDecl, HandlePresence, HandleTarget,
    IncomingParam, IntoRust, Native, OutOfRust, ParamPlan, ParamPlanRender, Primitive,
    ReturnPlanRender, ReturnValueSlot, TypeRef,
};

use crate::{
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        name_style::Name,
        render::{native::NativeCall, primitive::KotlinPrimitive, type_name::ParameterType},
        syntax::{Expression, Identifier, Statement, TypeName},
    },
};

const KOTLIN_TARGET: &str = "kotlin";

#[derive(AskamaTemplate)]
#[template(path = "target/kotlin/function.kt", escape = "none")]
struct FunctionTemplate {
    function: Function,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    name: Identifier,
    parameters: Vec<Parameter>,
    returns: Option<TypeName>,
    body: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeName,
    native_argument: Expression,
}

struct FunctionReturn {
    ty: Option<TypeName>,
    conversion: ReturnConversion,
}

enum ReturnConversion {
    Void,
    Direct(Primitive),
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Native>,
        _context: &RenderContext<Native>,
    ) -> Result<Self> {
        if !matches!(decl.callable().execution(), ExecutionDecl::Synchronous(_)) {
            return Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "async function",
            });
        }

        let parameters = decl
            .callable()
            .params()
            .iter()
            .map(Parameter::from_declaration)
            .collect::<Result<Vec<_>>>()?;
        let function_return = decl
            .callable()
            .returns()
            .plan()
            .render_with(&mut FunctionReturnPlan)?;
        let call = NativeCall::new(
            Identifier::escape(decl.symbol().name().as_str())?,
            parameters
                .iter()
                .map(|parameter| parameter.native_argument().clone())
                .collect(),
        );
        let body = function_return.body(call.expression())?;
        Ok(Self {
            name: Name::new(decl.name()).function()?,
            parameters,
            returns: function_return.ty,
            body,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        Ok(Emitted::primary(
            FunctionTemplate { function: self }.render()?,
        ))
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    pub fn returns(&self) -> Option<&TypeName> {
        self.returns.as_ref()
    }

    pub fn body(&self) -> &[Statement] {
        &self.body
    }
}

impl Parameter {
    pub fn from_declaration(
        parameter: &boltffi_binding::ParamDecl<Native, boltffi_binding::IntoRust>,
    ) -> Result<Self> {
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "closure function parameter",
            });
        };
        let name = Name::new(parameter.name()).parameter()?;
        Ok(Self {
            native_argument: Self::native_argument_for(name.clone(), plan)?,
            name,
            ty: Self::type_name(plan)?,
        })
    }

    pub fn name(&self) -> &Identifier {
        &self.name
    }

    pub fn ty(&self) -> &TypeName {
        &self.ty
    }

    fn native_argument(&self) -> &Expression {
        &self.native_argument
    }

    fn type_name(plan: &ParamPlan<Native, boltffi_binding::IntoRust>) -> Result<TypeName> {
        plan.render_with(&mut ParameterType)
    }
}

impl Parameter {
    fn native_argument_for(
        name: Identifier,
        plan: &ParamPlan<Native, boltffi_binding::IntoRust>,
    ) -> Result<Expression> {
        plan.render_with(&mut NativeArgument { name })
    }
}

struct NativeArgument {
    name: Identifier,
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for NativeArgument {
    type Output = Result<Expression>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match ty {
            DirectValueType::Primitive(primitive) => {
                KotlinPrimitive::new(*primitive).native_argument(value)
            }
            DirectValueType::Record(_) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "direct record function parameter",
            }),
            DirectValueType::Enum(_) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "direct enum function parameter",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown direct function parameter",
            }),
        }
    }

    fn encoded(
        &mut self,
        _ty: &'plan TypeRef,
        _codec: &'plan <IntoRust as Direction>::Codec,
        _shape: <Native as boltffi_binding::Surface>::BufferShape,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "encoded function parameter",
        })
    }

    fn handle(
        &mut self,
        _target: &'plan HandleTarget,
        _carrier: <Native as boltffi_binding::Surface>::HandleCarrier,
        _presence: HandlePresence,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "handle function parameter",
        })
    }

    fn scalar_option(&mut self, _primitive: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "optional scalar function parameter",
        })
    }

    fn direct_vector(
        &mut self,
        _element: &'plan boltffi_binding::DirectVectorElementType,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "direct-vector function parameter",
        })
    }
}

struct FunctionReturnPlan;

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for FunctionReturnPlan {
    type Output = Result<FunctionReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(FunctionReturn::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match (slot, ty) {
            (ReturnValueSlot::ReturnSlot, DirectValueType::Primitive(primitive)) => {
                FunctionReturn::direct(*primitive)
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Record(_)) => {
                Err(Error::UnsupportedTarget {
                    target: KOTLIN_TARGET,
                    shape: "direct record function return",
                })
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Enum(_)) => {
                Err(Error::UnsupportedTarget {
                    target: KOTLIN_TARGET,
                    shape: "direct enum function return",
                })
            }
            (ReturnValueSlot::OutPointer, _) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "out-pointer function return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown direct function return",
            }),
        }
    }

    fn encoded(
        &mut self,
        _slot: ReturnValueSlot,
        _ty: &'plan TypeRef,
        _codec: &'plan <OutOfRust as Direction>::Codec,
        _shape: <Native as boltffi_binding::Surface>::BufferShape,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "encoded function return",
        })
    }

    fn handle(
        &mut self,
        _slot: ReturnValueSlot,
        _target: &'plan HandleTarget,
        _carrier: <Native as boltffi_binding::Surface>::HandleCarrier,
        _presence: HandlePresence,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "handle function return",
        })
    }

    fn scalar_option(&mut self, _primitive: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "optional scalar function return",
        })
    }

    fn direct_vector(
        &mut self,
        _element: &'plan boltffi_binding::DirectVectorElementType,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "direct-vector function return",
        })
    }

    fn closure(
        &mut self,
        _closure: &'plan boltffi_binding::ClosureReturn<Native, OutOfRust>,
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "closure function return",
        })
    }
}

impl FunctionReturn {
    fn void() -> Self {
        Self {
            ty: None,
            conversion: ReturnConversion::Void,
        }
    }

    fn direct(primitive: Primitive) -> Result<Self> {
        let ty = KotlinPrimitive::new(primitive).api_type()?;
        Ok(Self {
            ty: Some(ty),
            conversion: ReturnConversion::Direct(primitive),
        })
    }

    fn body(&self, call: Expression) -> Result<Vec<Statement>> {
        match &self.conversion {
            ReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            ReturnConversion::Direct(primitive) => Ok(vec![Statement::return_value(
                KotlinPrimitive::new(*primitive).public_return(call)?,
            )]),
        }
    }
}

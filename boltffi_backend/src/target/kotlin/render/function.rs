use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClosureReturn, DirectValueType, DirectVectorElementType, Direction, EnumId, ExecutionDecl,
    FunctionDecl, HandlePresence, HandleTarget, IncomingParam, IntoRust, Native, OutOfRust,
    ParamDecl, ParamPlan, ParamPlanRender, Primitive, RecordId, ReturnPlanRender, ReturnValueSlot,
    Surface, TypeRef,
};

use crate::{
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        codec::{EncodedWrite, Reader, WireBuffer},
        name_style::Name,
        render::{
            enumeration::Enumeration,
            native::NativeCall,
            primitive::KotlinPrimitive,
            record::Record,
            type_name::{KotlinType, ParameterType},
        },
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
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
    setup: Vec<Statement>,
    call: Vec<Statement>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeName,
    native_argument: Expression,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

struct NativeArgument {
    expression: Expression,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

struct FunctionReturn {
    ty: Option<TypeName>,
    conversion: ReturnConversion,
}

enum ReturnConversion {
    Void,
    Direct(Primitive),
    DirectRecord(TypeName),
    DirectEnum { ty: TypeName, repr: Primitive },
    Encoded(<OutOfRust as Direction>::Codec),
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Native>,
        context: &RenderContext<Native>,
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
            .map(|parameter| Parameter::from_declaration(parameter, context))
            .collect::<Result<Vec<_>>>()?;
        let function_return = decl
            .callable()
            .returns()
            .plan()
            .render_with(&mut FunctionReturnPlan::new(context))?;
        let call = NativeCall::new(
            Identifier::escape(decl.symbol().name().as_str())?,
            parameters
                .iter()
                .map(|parameter| parameter.native_argument().clone())
                .collect(),
        );
        let call = function_return.statements(call.expression())?;
        let setup = parameters
            .iter()
            .flat_map(|parameter| parameter.setup().iter().cloned())
            .collect();
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup().iter().cloned())
            .collect();
        Ok(Self {
            name: Name::new(decl.name()).function()?,
            parameters,
            returns: function_return.ty,
            setup,
            call,
            cleanup,
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

    pub fn setup(&self) -> &[Statement] {
        &self.setup
    }

    pub fn call(&self) -> &[Statement] {
        &self.call
    }

    pub fn cleanup(&self) -> &[Statement] {
        &self.cleanup
    }

    pub fn has_cleanup(&self) -> bool {
        !self.cleanup.is_empty()
    }
}

impl Parameter {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "closure function parameter",
            });
        };
        let source_name = Name::new(parameter.name());
        let name = source_name.parameter()?;
        let native_argument = Self::native_argument_for(source_name, name.clone(), plan, context)?;
        Ok(Self {
            native_argument: native_argument.expression,
            name,
            ty: Self::type_name(plan, context)?,
            setup: native_argument.setup,
            cleanup: native_argument.cleanup,
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

    fn setup(&self) -> &[Statement] {
        &self.setup
    }

    fn cleanup(&self) -> &[Statement] {
        &self.cleanup
    }

    fn type_name(
        plan: &ParamPlan<Native, IntoRust>,
        context: &RenderContext<Native>,
    ) -> Result<TypeName> {
        plan.render_with(&mut ParameterType::new(context))
    }
}

impl Parameter {
    fn native_argument_for(
        source_name: Name,
        name: Identifier,
        plan: &ParamPlan<Native, IntoRust>,
        context: &RenderContext<Native>,
    ) -> Result<NativeArgument> {
        plan.render_with(&mut NativeArgumentRender {
            source_name,
            name,
            context,
        })
    }
}

struct NativeArgumentRender<'context> {
    source_name: Name,
    name: Identifier,
    context: &'context RenderContext<'context, Native>,
}

impl NativeArgument {
    fn direct(expression: Expression) -> Self {
        Self {
            expression,
            setup: Vec::new(),
            cleanup: Vec::new(),
        }
    }

    fn encoded(write: EncodedWrite) -> Self {
        let (setup, expression, cleanup) = write.into_parts();
        Self {
            expression,
            setup,
            cleanup,
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for NativeArgumentRender<'_> {
    type Output = Result<NativeArgument>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        let value = Expression::identifier(self.name.clone());
        match ty {
            DirectValueType::Primitive(primitive) => KotlinPrimitive::new(*primitive)
                .native_argument(value)
                .map(NativeArgument::direct),
            DirectValueType::Record(_) => {
                Record::encode_expression(value).map(NativeArgument::direct)
            }
            DirectValueType::Enum(enumeration) => {
                Enumeration::native_argument(*enumeration, value, self.context)
                    .map(NativeArgument::direct)
            }
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown direct function parameter",
            }),
        }
    }

    fn encoded(
        &mut self,
        _ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        WireBuffer::new(&self.source_name)
            .and_then(|buffer| buffer.write(codec))
            .map(NativeArgument::encoded)
    }

    fn handle(
        &mut self,
        _target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
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

    fn direct_vector(&mut self, _element: &'plan DirectVectorElementType) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "direct-vector function parameter",
        })
    }
}

struct FunctionReturnPlan<'context> {
    context: &'context RenderContext<'context, Native>,
}

impl<'context> FunctionReturnPlan<'context> {
    fn new(context: &'context RenderContext<'context, Native>) -> Self {
        Self { context }
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for FunctionReturnPlan<'_> {
    type Output = Result<FunctionReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(FunctionReturn::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match (slot, ty) {
            (ReturnValueSlot::ReturnSlot, DirectValueType::Primitive(primitive)) => {
                FunctionReturn::direct(*primitive)
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Record(record)) => {
                FunctionReturn::direct_record(*record, self.context)
            }
            (ReturnValueSlot::ReturnSlot, DirectValueType::Enum(enumeration)) => {
                FunctionReturn::direct_enum(*enumeration, self.context)
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
        slot: ReturnValueSlot,
        ty: &'plan TypeRef,
        codec: &'plan <OutOfRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
    ) -> Self::Output {
        match slot {
            ReturnValueSlot::ReturnSlot => FunctionReturn::encoded(ty, codec.clone(), self.context),
            ReturnValueSlot::OutPointer => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "out-pointer encoded function return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown encoded function return",
            }),
        }
    }

    fn handle(
        &mut self,
        _slot: ReturnValueSlot,
        _target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
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

    fn direct_vector(&mut self, _element: &'plan DirectVectorElementType) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "direct-vector function return",
        })
    }

    fn closure(&mut self, _closure: &'plan ClosureReturn<Native, OutOfRust>) -> Self::Output {
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

    fn direct_record(record: RecordId, context: &RenderContext<Native>) -> Result<Self> {
        let ty = Record::type_name_from_id(record, context)?;
        Ok(Self {
            ty: Some(ty.clone()),
            conversion: ReturnConversion::DirectRecord(ty),
        })
    }

    fn direct_enum(enumeration: EnumId, context: &RenderContext<Native>) -> Result<Self> {
        let enumeration = Enumeration::from_id(enumeration, context)?;
        let ty = enumeration.name().clone();
        Ok(Self {
            ty: Some(ty.clone()),
            conversion: ReturnConversion::DirectEnum {
                ty,
                repr: enumeration.repr(),
            },
        })
    }

    fn encoded(
        ty: &TypeRef,
        codec: <OutOfRust as Direction>::Codec,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Ok(Self {
            ty: Some(KotlinType::type_ref(ty, context)?),
            conversion: ReturnConversion::Encoded(codec),
        })
    }

    fn statements(&self, call: Expression) -> Result<Vec<Statement>> {
        match &self.conversion {
            ReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            ReturnConversion::Direct(primitive) => Ok(vec![
                KotlinPrimitive::new(*primitive)
                    .public_return(call)
                    .map(Statement::return_value)?,
            ]),
            ReturnConversion::DirectRecord(record) => {
                let result = Identifier::parse("__boltffi_result")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                Ok(vec![
                    Statement::value(result.clone(), payload),
                    Statement::return_value(Record::decode_expression(
                        record.clone(),
                        Expression::identifier(result),
                    )?),
                ])
            }
            ReturnConversion::DirectEnum { ty, repr } => {
                let value = KotlinPrimitive::new(*repr).public_return(call)?;
                Ok(vec![Statement::return_value(Expression::call(
                    ty.clone(),
                    Identifier::parse("fromValue")?,
                    [value].into_iter().collect::<ArgumentList>(),
                ))])
            }
            ReturnConversion::Encoded(codec) => {
                let result = Identifier::parse("__boltffi_result")?;
                let reader = Identifier::parse("__boltffi_reader")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                let value = codec.render_with(&mut Reader::new(reader.clone()))?;
                Ok(vec![
                    Statement::value(result.clone(), payload),
                    Statement::value(
                        reader,
                        Expression::construct(
                            TypeName::new("WireReader"),
                            [Expression::identifier(result)]
                                .into_iter()
                                .collect::<ArgumentList>(),
                        ),
                    ),
                    Statement::return_value(value),
                ])
            }
        }
    }
}

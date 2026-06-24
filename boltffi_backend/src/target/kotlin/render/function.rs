use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClassId, ClosureReturn, DirectValueType, DirectVectorElementType, Direction, EnumId,
    ErrorChannel, ErrorPlacement, ExecutionDecl, ExportedCallable, FunctionDecl, HandlePresence,
    HandleTarget, IncomingParam, IntoRust, Native, NativeSymbol, OutOfRust, ParamDecl, ParamPlan,
    ParamPlanRender, Primitive, RecordId, ReturnPlanRender, ReturnValueSlot, Surface, TypeRef,
};

use crate::{
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        codec::{EncodedWrite, Reader, ScalarOption, WireBuffer},
        name_style::Name,
        primitive::KotlinPrimitive,
        render::{
            class::ClassHandle,
            direct_vector::DirectVector,
            enumeration::Enumeration,
            native::NativeCall,
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
pub struct ExportedCall {
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
    DirectVector(DirectVector),
    Encoded(<OutOfRust as Direction>::Codec),
    ClassHandle(ClassHandle),
    ScalarOption(Primitive),
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Native>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        ExportedCall::new(
            Name::new(decl.name()).function()?,
            decl.symbol(),
            decl.callable(),
            Vec::new(),
            context,
        )
        .map(Self::from_call)
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

    fn from_call(call: ExportedCall) -> Self {
        Self {
            name: call.name,
            parameters: call.parameters,
            returns: call.returns,
            setup: call.setup,
            call: call.call,
            cleanup: call.cleanup,
        }
    }
}

impl ExportedCall {
    pub fn new(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if !matches!(callable.execution(), ExecutionDecl::Synchronous(_)) {
            return Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "async function",
            });
        }

        let parameters = callable
            .params()
            .iter()
            .map(|parameter| Parameter::from_declaration(parameter, context))
            .collect::<Result<Vec<_>>>()?;
        let function_return = callable
            .returns()
            .plan()
            .render_with(&mut FunctionReturnPlan::new(context, callable))?;
        let native_arguments = native_prefix
            .into_iter()
            .chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.native_argument().clone()),
            )
            .collect();
        let native_call = NativeCall::new(
            Identifier::escape(symbol.name().as_str())?,
            native_arguments,
        );
        let setup = parameters
            .iter()
            .flat_map(|parameter| parameter.setup().iter().cloned())
            .collect();
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup().iter().cloned())
            .collect();
        let returns = function_return.ty.clone();
        let call = function_return.statements(native_call.expression(), context)?;
        Ok(Self {
            name,
            parameters,
            returns,
            setup,
            call,
            cleanup,
        })
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
            .and_then(|buffer| buffer.write(codec, self.context))
            .map(NativeArgument::encoded)
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => ClassHandle::new(*class, presence, self.context)
                .and_then(|handle| {
                    handle
                        .parameter_argument(Expression::identifier(self.name.clone()))
                        .map(NativeArgument::direct)
                }),
            HandleTarget::Callback(_) | HandleTarget::Stream(_) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "handle function parameter",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown handle function parameter",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        ScalarOption::new(primitive)
            .write(&self.source_name)
            .map(NativeArgument::encoded)
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        DirectVector::from_element(element).map(|vector| {
            NativeArgument::direct(
                vector.carrier_expression(Expression::identifier(self.name.clone())),
            )
        })
    }
}

struct FunctionReturnPlan<'context> {
    context: &'context RenderContext<'context, Native>,
    fallible_success_out: bool,
}

impl<'context> FunctionReturnPlan<'context> {
    fn new(
        context: &'context RenderContext<'context, Native>,
        callable: &ExportedCallable<Native>,
    ) -> Self {
        let error_channel = callable.error().channel();
        Self {
            context,
            fallible_success_out: matches!(
                error_channel,
                ErrorChannel::Status
                    | ErrorChannel::Encoded {
                        placement: ErrorPlacement::ReturnSlot,
                        ..
                    }
            ),
        }
    }

    fn require_supported_slot(&self, slot: ReturnValueSlot, shape: &'static str) -> Result<()> {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(()),
            ReturnValueSlot::OutPointer if self.fallible_success_out => Ok(()),
            ReturnValueSlot::OutPointer => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape,
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown function return slot",
            }),
        }
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for FunctionReturnPlan<'_> {
    type Output = Result<FunctionReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(FunctionReturn::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        self.require_supported_slot(slot, "out-pointer function return")?;
        match (slot, ty) {
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Primitive(primitive),
            ) => FunctionReturn::direct(*primitive),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Record(record),
            ) => FunctionReturn::direct_record(*record, self.context),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Enum(enumeration),
            ) => FunctionReturn::direct_enum(*enumeration, self.context),
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
        self.require_supported_slot(slot, "out-pointer encoded function return")?;
        match slot {
            ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer => {
                FunctionReturn::encoded(ty, codec.clone(), self.context)
            }
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown encoded function return",
            }),
        }
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        self.require_supported_slot(slot, "out-pointer handle function return")?;
        match (slot, target) {
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                HandleTarget::Class(class),
            ) => FunctionReturn::class_handle(*class, presence, self.context),
            (_, HandleTarget::Callback(_) | HandleTarget::Stream(_)) => {
                Err(Error::UnsupportedTarget {
                    target: KOTLIN_TARGET,
                    shape: "handle function return",
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown handle function return",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        FunctionReturn::scalar_option(primitive)
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        FunctionReturn::direct_vector(element)
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
                repr: enumeration.repr()?,
            },
        })
    }

    fn direct_vector(element: &DirectVectorElementType) -> Result<Self> {
        let vector = DirectVector::from_element(element)?;
        Ok(Self {
            ty: Some(vector.ty().clone()),
            conversion: ReturnConversion::DirectVector(vector),
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

    fn class_handle(
        class: ClassId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let handle = ClassHandle::new(class, presence, context)?;
        let ty = handle.ty()?;
        Ok(Self {
            ty: Some(ty),
            conversion: ReturnConversion::ClassHandle(handle),
        })
    }

    fn scalar_option(primitive: Primitive) -> Result<Self> {
        Ok(Self {
            ty: Some(ScalarOption::new(primitive).ty()?),
            conversion: ReturnConversion::ScalarOption(primitive),
        })
    }

    fn statements(
        &self,
        call: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
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
            ReturnConversion::DirectVector(vector) => vector.return_statements(call),
            ReturnConversion::Encoded(codec) => {
                let result = Identifier::parse("__boltffi_result")?;
                let reader = Identifier::parse("__boltffi_reader")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                let value = codec.render_with(&mut Reader::new(reader.clone(), context))?;
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
            ReturnConversion::ClassHandle(handle) => handle.return_statements(call),
            ReturnConversion::ScalarOption(primitive) => ScalarOption::new(*primitive).read(call),
        }
    }
}

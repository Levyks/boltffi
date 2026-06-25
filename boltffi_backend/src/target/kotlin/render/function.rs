use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackId, ClassId, ClosureReturn, DirectValueType, DirectVectorElementType, Direction,
    EnumId, ErrorChannel, ErrorPlacement, ExecutionDecl, ExportedCallable, FunctionDecl,
    HandlePresence, HandleTarget, IncomingParam, IntoRust, Native, NativeSymbol, OutOfRust,
    ParamDecl, ParamPlan, ParamPlanRender, Primitive, Receive, RecordId, ReturnPlanRender,
    ReturnValueSlot, Surface, TypeRef, native,
};

use crate::{
    bridge::jni::JniBridgeContract,
    core::{Emitted, Error, RenderContext, Result},
    target::kotlin::{
        KotlinHost,
        codec::{EncodedWrite, Reader, ScalarOption, WireBuffer},
        name_style::{KotlinPackage, Name},
        primitive::KotlinPrimitive,
        render::{
            callback::CallbackHandle,
            class::ClassHandle,
            closure::Closure,
            direct_vector::DirectVector,
            enumeration::Enumeration,
            native::NativeCall,
            record::Record,
            type_name::{KotlinType, ParameterType},
        },
        syntax::{ArgumentList, Expression, Identifier, Literal, Statement, TypeName},
    },
};

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
    async_call: Option<AsyncCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportedCall {
    name: Identifier,
    parameters: Vec<Parameter>,
    returns: Option<TypeName>,
    setup: Vec<Statement>,
    call: Vec<Statement>,
    cleanup: Vec<Statement>,
    async_call: Option<AsyncCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    name: Identifier,
    ty: TypeName,
    native_argument: Expression,
    mutation: Option<ParameterMutation>,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

struct NativeArgument {
    expression: Expression,
    mutation: Option<ParameterMutation>,
    setup: Vec<Statement>,
    cleanup: Vec<Statement>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParameterMutation {
    destination: Identifier,
    reader: Identifier,
    result: Identifier,
    read: <IntoRust as Direction>::Codec,
}

struct FunctionReturn {
    ty: Option<TypeName>,
    conversion: ReturnConversion,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedReceiverMutation {
    ty: TypeName,
    record_package: Option<KotlinPackage>,
}

enum ReceiverBinding {
    None,
    RecordPackage(KotlinPackage),
    EncodedMutation(EncodedReceiverMutation),
}

impl EncodedReceiverMutation {
    pub fn new(ty: TypeName) -> Self {
        Self {
            ty,
            record_package: None,
        }
    }

    pub fn with_record_package(mut self, record_package: &KotlinPackage) -> Self {
        self.record_package = Some(record_package.clone());
        self
    }

    fn record_package(&self) -> Option<&KotlinPackage> {
        self.record_package.as_ref()
    }
}

impl ReceiverBinding {
    fn record_package(&self) -> Option<&KotlinPackage> {
        match self {
            Self::None => None,
            Self::RecordPackage(record_package) => Some(record_package),
            Self::EncodedMutation(receiver) => receiver.record_package(),
        }
    }

    fn into_mutation(self) -> Option<EncodedReceiverMutation> {
        match self {
            Self::EncodedMutation(receiver) => Some(receiver),
            Self::None | Self::RecordPackage(_) => None,
        }
    }
}

enum ErrorConversion {
    None,
    Status,
    Encoded {
        ty: TypeRef,
        codec: <OutOfRust as Direction>::Codec,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AsyncCall {
    create_setup: Vec<Statement>,
    create: Expression,
    create_cleanup: Vec<Statement>,
    poll: Identifier,
    complete_body: Vec<Statement>,
    free: Identifier,
    cancel: Identifier,
    returns_value: bool,
}

struct AsyncProtocolFunctions {
    poll: Identifier,
    complete: Identifier,
    cancel: Identifier,
    free: Identifier,
}

enum ReturnConversion {
    Void,
    Direct(Primitive),
    ByteArrayValue(TypeName),
    DirectEnum(TypeName),
    DirectVector(DirectVector),
    Encoded {
        codec: <OutOfRust as Direction>::Codec,
        record_package: Option<KotlinPackage>,
    },
    ClassHandle(ClassHandle),
    CallbackHandle(CallbackHandle),
    ScalarOption(Primitive),
    ParameterMutation(ParameterMutation),
}

impl Function {
    pub fn from_declaration(
        decl: &FunctionDecl<Native>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        ExportedCall::new(
            Name::new(decl.name()).function()?,
            decl.symbol(),
            decl.callable(),
            Vec::new(),
            bridge,
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

    pub fn async_call(&self) -> Option<&AsyncCall> {
        self.async_call.as_ref()
    }

    fn from_call(call: ExportedCall) -> Self {
        Self {
            name: call.name,
            parameters: call.parameters,
            returns: call.returns,
            setup: call.setup,
            call: call.call,
            cleanup: call.cleanup,
            async_call: call.async_call,
        }
    }
}

impl ExportedCall {
    pub fn new(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build(
            name,
            symbol,
            callable,
            native_prefix,
            ReceiverBinding::None,
            bridge,
            context,
        )
    }

    pub fn new_with_record_package(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        record_package: &KotlinPackage,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build(
            name,
            symbol,
            callable,
            native_prefix,
            ReceiverBinding::RecordPackage(record_package.clone()),
            bridge,
            context,
        )
    }

    pub fn new_encoded_receiver_mutation(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        receiver_mutation: EncodedReceiverMutation,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::build(
            name,
            symbol,
            callable,
            native_prefix,
            ReceiverBinding::EncodedMutation(receiver_mutation),
            bridge,
            context,
        )
    }

    fn build(
        name: Identifier,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        native_prefix: Vec<Expression>,
        receiver: ReceiverBinding,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let record_package = receiver.record_package();
        let parameters = callable
            .params()
            .iter()
            .map(|parameter| {
                Parameter::from_declaration(parameter, record_package, bridge, context)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut function_return =
            callable
                .returns()
                .plan()
                .render_with(&mut FunctionReturnPlan::new(
                    context,
                    record_package,
                    callable,
                ))?;
        if let Some(receiver_mutation) = receiver.into_mutation() {
            function_return =
                function_return.with_byte_array_receiver_writeback(receiver_mutation.ty)?;
        }
        if let Some(mutation) = Self::parameter_mutation(&parameters)? {
            function_return = function_return.with_parameter_mutation(mutation)?;
        }
        let native_arguments = native_prefix
            .into_iter()
            .chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.native_argument().clone()),
            )
            .collect::<Vec<_>>();
        let native_call = NativeCall::new(
            Identifier::escape(symbol.name().as_str())?,
            native_arguments,
        );
        let error_conversion = ErrorConversion::from_channel(callable.error().channel())?;
        let setup = parameters
            .iter()
            .flat_map(|parameter| parameter.setup().iter().cloned())
            .collect::<Vec<_>>();
        let cleanup = parameters
            .iter()
            .flat_map(|parameter| parameter.cleanup().iter().cloned())
            .collect::<Vec<_>>();
        let returns = function_return.ty.clone();
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => Ok(Self {
                name,
                parameters,
                returns,
                setup,
                call: function_return.return_statements(
                    error_conversion.wrap(native_call.expression(), context)?,
                    context,
                )?,
                cleanup,
                async_call: None,
            }),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                ..
            }) => Ok(Self {
                name,
                parameters,
                returns,
                setup: Vec::new(),
                call: Vec::new(),
                cleanup: Vec::new(),
                async_call: Some(AsyncCall::new(
                    native_call.expression(),
                    setup,
                    cleanup,
                    AsyncProtocolFunctions::new(poll, complete, cancel, free)?,
                    function_return,
                    error_conversion,
                    context,
                )?),
            }),
            ExecutionDecl::Asynchronous(_) => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unsupported async function protocol",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown function execution",
            }),
        }
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

    pub fn async_call(&self) -> Option<&AsyncCall> {
        self.async_call.as_ref()
    }

    fn parameter_mutation(parameters: &[Parameter]) -> Result<Option<ParameterMutation>> {
        let mut mutations = parameters.iter().filter_map(Parameter::mutation).cloned();
        let mutation = mutations.next();
        if mutations.next().is_some() {
            return Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "multiple mutable encoded parameters",
            });
        }
        Ok(mutation)
    }
}

impl Parameter {
    pub fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        record_package: Option<&KotlinPackage>,
        bridge: &JniBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let source_name = Name::new(parameter.name());
        let name = source_name.parameter()?;
        let (ty, native_argument) = match parameter.payload() {
            IncomingParam::Value(plan) => (
                Self::type_name(plan, record_package, context)?,
                Self::native_argument_for(source_name, name.clone(), plan, context)?,
            ),
            IncomingParam::Closure(closure) => (
                Closure::type_name(closure, context)?,
                NativeArgument::direct(Closure::native_argument(
                    closure,
                    Expression::identifier(name.clone()),
                    bridge,
                )?),
            ),
        };
        Ok(Self {
            native_argument: native_argument.expression,
            mutation: native_argument.mutation,
            name,
            ty,
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

    fn mutation(&self) -> Option<&ParameterMutation> {
        self.mutation.as_ref()
    }

    fn type_name(
        plan: &ParamPlan<Native, IntoRust>,
        record_package: Option<&KotlinPackage>,
        context: &RenderContext<Native>,
    ) -> Result<TypeName> {
        plan.render_with(&mut ParameterType::new(context).record_package(record_package))
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
            mutation: None,
            setup: Vec::new(),
            cleanup: Vec::new(),
        }
    }

    fn encoded(write: EncodedWrite, mutation: Option<ParameterMutation>) -> Self {
        let (setup, expression, cleanup) = write.into_parts();
        Self {
            expression,
            mutation,
            setup,
            cleanup,
        }
    }
}

impl ParameterMutation {
    fn from_encoded(
        source_name: &Name,
        destination: Identifier,
        ty: &TypeRef,
        codec: &<IntoRust as Direction>::Codec,
        shape: native::BufferShape,
        receive: Receive,
    ) -> Result<Option<Self>> {
        if receive != Receive::ByMutRef {
            return Ok(None);
        }
        match (shape, ty) {
            (native::BufferShape::Slice, TypeRef::Bytes) => {
                Self::new(source_name, destination, codec).map(Some)
            }
            (native::BufferShape::Slice, TypeRef::Sequence(inner))
                if matches!(inner.as_ref(), TypeRef::Primitive(_)) =>
            {
                Self::new(source_name, destination, codec).map(Some)
            }
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "mutable encoded parameter",
            }),
        }
    }

    fn new(
        source_name: &Name,
        destination: Identifier,
        codec: &<IntoRust as Direction>::Codec,
    ) -> Result<Self> {
        Ok(Self {
            destination,
            reader: source_name.generated("mutation_reader")?,
            result: source_name.generated("mutation")?,
            read: codec.clone(),
        })
    }

    fn statements(
        &self,
        call: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
            "null mutation buffer returned",
        )));
        let mut reader = Reader::new(self.reader.clone(), context);
        let decoded = self
            .read
            .read_plan()
            .render_with(&mut reader)?
            .into_expression();
        Ok(vec![
            Statement::value(self.result.clone(), payload),
            Statement::value(
                self.reader.clone(),
                Expression::construct(
                    TypeName::new("WireReader"),
                    [Expression::identifier(self.result.clone())]
                        .into_iter()
                        .collect::<ArgumentList>(),
                ),
            ),
            Statement::expression(Expression::call(
                decoded,
                Identifier::parse("copyInto")?,
                [Expression::identifier(self.destination.clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            )),
        ])
    }
}

impl AsyncCall {
    fn new(
        create: Expression,
        create_setup: Vec<Statement>,
        create_cleanup: Vec<Statement>,
        functions: AsyncProtocolFunctions,
        returns: FunctionReturn,
        error_conversion: ErrorConversion,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let future = Expression::identifier(Identifier::parse("future")?);
        let complete_call = error_conversion.wrap(
            NativeCall::new(functions.complete.clone(), vec![future]).expression(),
            context,
        )?;
        Ok(Self {
            create_setup,
            create,
            create_cleanup,
            poll: functions.poll,
            complete_body: returns.value_statements(complete_call, context)?,
            free: functions.free,
            cancel: functions.cancel,
            returns_value: returns.ty.is_some(),
        })
    }

    pub fn create_setup(&self) -> &[Statement] {
        &self.create_setup
    }

    pub fn create(&self) -> &Expression {
        &self.create
    }

    pub fn create_cleanup(&self) -> &[Statement] {
        &self.create_cleanup
    }

    pub fn has_create_cleanup(&self) -> bool {
        !self.create_cleanup.is_empty()
    }

    pub fn poll(&self) -> &Identifier {
        &self.poll
    }

    pub fn complete_body(&self) -> &[Statement] {
        &self.complete_body
    }

    pub fn free(&self) -> &Identifier {
        &self.free
    }

    pub fn cancel(&self) -> &Identifier {
        &self.cancel
    }

    pub fn returns_value(&self) -> bool {
        self.returns_value
    }
}

impl ErrorConversion {
    fn from_channel(channel: ErrorChannel<'_, Native, OutOfRust>) -> Result<Self> {
        match channel {
            ErrorChannel::None => Ok(Self::None),
            ErrorChannel::Status => Ok(Self::Status),
            ErrorChannel::Encoded {
                placement: ErrorPlacement::ReturnSlot,
                ty,
                codec,
                ..
            } => Ok(Self::Encoded {
                ty: ty.clone(),
                codec: codec.clone(),
            }),
            ErrorChannel::Encoded { .. } => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "encoded error out-pointer",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown function error channel",
            }),
        }
    }

    fn wrap(&self, call: Expression, context: &RenderContext<Native>) -> Result<Expression> {
        match self {
            Self::None => Ok(call),
            Self::Status => self.status(call),
            Self::Encoded { ty, codec } => self.encoded(call, ty, codec, context),
        }
    }

    fn status(&self, call: Expression) -> Result<Expression> {
        let error = Identifier::parse("__boltffi_error")?;
        let message = Expression::property(
            Expression::identifier(error.clone()),
            Identifier::parse("message")?,
        )
        .or_else(Expression::literal(Literal::string("BoltFFI call failed")));
        Ok(call.try_catch(
            error,
            TypeName::new("RuntimeException"),
            Expression::throwing(Expression::construct(
                TypeName::new("FfiException"),
                [message].into_iter().collect::<ArgumentList>(),
            )),
        ))
    }

    fn encoded(
        &self,
        call: Expression,
        ty: &TypeRef,
        codec: &<OutOfRust as Direction>::Codec,
        context: &RenderContext<Native>,
    ) -> Result<Expression> {
        let error = Identifier::parse("__boltffi_error")?;
        let reader = Identifier::parse("__boltffi_error_reader")?;
        let mut codec_reader = Reader::new(reader.clone(), context);
        let decoded = codec.render_with(&mut codec_reader)?.into_expression();
        let thrown = match ty {
            TypeRef::String => Expression::construct(
                TypeName::new("FfiException"),
                [decoded].into_iter().collect::<ArgumentList>(),
            ),
            TypeRef::Record(_) | TypeRef::Enum(_) => decoded,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: KotlinHost::TARGET,
                    shape: "kotlin throwable error type",
                });
            }
        };
        Ok(call.try_catch(
            error.clone(),
            TypeName::new("BoltFfiErrorBufferException"),
            Expression::run(
                vec![Statement::value(
                    reader,
                    Expression::construct(
                        TypeName::new("WireReader"),
                        [Expression::property(
                            Expression::identifier(error),
                            Identifier::parse("bytes")?,
                        )]
                        .into_iter()
                        .collect::<ArgumentList>(),
                    ),
                )],
                Expression::throwing(thrown),
            ),
        ))
    }
}

impl AsyncProtocolFunctions {
    fn new(
        poll: &NativeSymbol,
        complete: &NativeSymbol,
        cancel: &NativeSymbol,
        free: &NativeSymbol,
    ) -> Result<Self> {
        Ok(Self {
            poll: Identifier::escape(poll.name().as_str())?,
            complete: Identifier::escape(complete.name().as_str())?,
            cancel: Identifier::escape(cancel.name().as_str())?,
            free: Identifier::escape(free.name().as_str())?,
        })
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
                target: KotlinHost::TARGET,
                shape: "unknown direct function parameter",
            }),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        codec: &'plan <IntoRust as Direction>::Codec,
        shape: <Native as Surface>::BufferShape,
        receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        let mutation = ParameterMutation::from_encoded(
            &self.source_name,
            self.name.clone(),
            ty,
            codec,
            shape,
            receive,
        )?;
        WireBuffer::new(&self.source_name)
            .and_then(|buffer| buffer.write(codec, self.context))
            .map(|write| NativeArgument::encoded(write, mutation))
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
            HandleTarget::Callback(callback) => {
                CallbackHandle::new(*callback, presence, self.context).and_then(|handle| {
                    handle
                        .parameter_argument(Expression::identifier(self.name.clone()))
                        .map(NativeArgument::direct)
                })
            }
            HandleTarget::Stream(_) => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "handle function parameter",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "unknown handle function parameter",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        ScalarOption::new(primitive)
            .write(&self.source_name)
            .map(|write| NativeArgument::encoded(write, None))
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        DirectVector::from_element(element)
            .map(|_| NativeArgument::direct(Expression::identifier(self.name.clone())))
    }
}

struct FunctionReturnPlan<'context> {
    context: &'context RenderContext<'context, Native>,
    record_package: Option<KotlinPackage>,
    fallible_success_out: bool,
}

impl<'context> FunctionReturnPlan<'context> {
    fn new(
        context: &'context RenderContext<'context, Native>,
        record_package: Option<&KotlinPackage>,
        callable: &ExportedCallable<Native>,
    ) -> Self {
        let error_channel = callable.error().channel();
        Self {
            context,
            record_package: record_package.cloned(),
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
                target: KotlinHost::TARGET,
                shape,
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
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
            ) => FunctionReturn::direct_record(*record, self.context, self.record_package.as_ref()),
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                DirectValueType::Enum(enumeration),
            ) => FunctionReturn::direct_enum(*enumeration, self.context),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
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
            ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer => FunctionReturn::encoded(
                ty,
                codec.clone(),
                self.context,
                self.record_package.as_ref(),
            ),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
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
            (
                ReturnValueSlot::ReturnSlot | ReturnValueSlot::OutPointer,
                HandleTarget::Callback(callback),
            ) => FunctionReturn::callback_handle(*callback, presence, self.context),
            (_, HandleTarget::Stream(_)) => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "handle function return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
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
            target: KotlinHost::TARGET,
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

    fn direct_record(
        record: RecordId,
        context: &RenderContext<Native>,
        record_package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        let ty = Record::type_name_from_id(record, context).map(|record| {
            record_package.map_or(record.clone(), |package| {
                TypeName::qualified(package, record)
            })
        })?;
        Ok(Self::byte_array_value(ty))
    }

    fn byte_array_value(ty: TypeName) -> Self {
        Self {
            ty: Some(ty.clone()),
            conversion: ReturnConversion::ByteArrayValue(ty),
        }
    }

    fn direct_enum(enumeration: EnumId, context: &RenderContext<Native>) -> Result<Self> {
        let enumeration = Enumeration::from_id(enumeration, context)?;
        let ty = enumeration.name().clone();
        Ok(Self {
            ty: Some(ty.clone()),
            conversion: ReturnConversion::DirectEnum(ty),
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
        record_package: Option<&KotlinPackage>,
    ) -> Result<Self> {
        Ok(Self {
            ty: Some(match record_package {
                Some(package) => KotlinType::type_ref_with_record_package(ty, context, package)?,
                None => KotlinType::type_ref(ty, context)?,
            }),
            conversion: ReturnConversion::Encoded {
                codec,
                record_package: record_package.cloned(),
            },
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

    fn callback_handle(
        callback: CallbackId,
        presence: HandlePresence,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let handle = CallbackHandle::new(callback, presence, context)?;
        let ty = handle.ty()?;
        Ok(Self {
            ty: Some(ty),
            conversion: ReturnConversion::CallbackHandle(handle),
        })
    }

    fn scalar_option(primitive: Primitive) -> Result<Self> {
        Ok(Self {
            ty: Some(ScalarOption::new(primitive).ty()?),
            conversion: ReturnConversion::ScalarOption(primitive),
        })
    }

    fn with_byte_array_receiver_writeback(self, receiver_type: TypeName) -> Result<Self> {
        match self.ty {
            None => Ok(Self::byte_array_value(receiver_type)),
            Some(_) => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "mutable receiver with explicit return",
            }),
        }
    }

    fn with_parameter_mutation(self, mutation: ParameterMutation) -> Result<Self> {
        match self.ty {
            None => Ok(Self {
                ty: None,
                conversion: ReturnConversion::ParameterMutation(mutation),
            }),
            Some(_) => Err(Error::UnsupportedTarget {
                target: KotlinHost::TARGET,
                shape: "mutable encoded parameter with explicit return",
            }),
        }
    }

    fn return_statements(
        &self,
        call: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        self.value_statements(call, context).map(|body| {
            if self.ty.is_some() {
                Statement::with_return_value(body)
            } else {
                body
            }
        })
    }

    fn value_statements(
        &self,
        call: Expression,
        context: &RenderContext<Native>,
    ) -> Result<Vec<Statement>> {
        match &self.conversion {
            ReturnConversion::Void => Ok(vec![Statement::expression(call)]),
            ReturnConversion::Direct(primitive) => Ok(vec![
                KotlinPrimitive::new(*primitive)
                    .public_return(call)
                    .map(Statement::expression)?,
            ]),
            ReturnConversion::ByteArrayValue(ty) => {
                let result = Identifier::parse("__boltffi_result")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                Ok(vec![
                    Statement::value(result.clone(), payload),
                    Statement::expression(Expression::call(
                        ty.clone(),
                        Identifier::parse("fromByteArray")?,
                        [Expression::identifier(result)]
                            .into_iter()
                            .collect::<ArgumentList>(),
                    )),
                ])
            }
            ReturnConversion::DirectEnum(ty) => Ok(vec![Statement::expression(Expression::call(
                ty.clone(),
                Identifier::parse("fromValue")?,
                [call].into_iter().collect::<ArgumentList>(),
            ))]),
            ReturnConversion::DirectVector(vector) => vector.value_statements(call),
            ReturnConversion::Encoded {
                codec,
                record_package,
            } => {
                let result = Identifier::parse("__boltffi_result")?;
                let reader = Identifier::parse("__boltffi_reader")?;
                let payload = call.or_else(Expression::throw_illegal_state(Literal::string(
                    "null buffer returned",
                )));
                let mut codec_reader = Reader::new(reader.clone(), context);
                if let Some(package) = record_package {
                    codec_reader = codec_reader.record_package(package);
                }
                let value = codec.render_with(&mut codec_reader)?.into_expression();
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
                    Statement::expression(value),
                ])
            }
            ReturnConversion::ClassHandle(handle) => handle.value_statements(call),
            ReturnConversion::CallbackHandle(handle) => handle.value_statements(call),
            ReturnConversion::ScalarOption(primitive) => {
                ScalarOption::new(*primitive).read_value(call)
            }
            ReturnConversion::ParameterMutation(mutation) => mutation.statements(call, context),
        }
    }
}

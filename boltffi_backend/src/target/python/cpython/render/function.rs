use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClosureReturn, ErrorDecl, ExecutionDecl, ExportedCallable, FunctionDecl, HandlePresence,
    HandleTarget, IncomingParam, IntoRust, Native, NativeSymbol, OutOfRust, ParamPlanRender,
    Primitive, ReadPlan, Receive, ReturnPlan, ReturnPlanRender, ReturnValueSlot, TypeRef,
    WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, syntax::TypeSyntax},
        python_cext::{ExtensionMethod, LoadedFunction, MethodFlags, PythonCExtBridgeContract},
    },
    core::{Diagnostic, Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{argument, direct_vector, primitive, result},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/function.c", escape = "none")]
struct SyncTemplate {
    python_name: String,
    wrapper: String,
    storage: String,
    params: Vec<argument::Conversion>,
    call_args: Vec<String>,
    returns: result::Conversion,
    mutation: Option<argument::MutationOutput>,
    fallible: Option<FallibleResult>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/async_function.c", escape = "none")]
struct AsyncTemplate {
    python_name: String,
    start_wrapper: String,
    start_storage: String,
    poll_python_name: String,
    poll_wrapper: String,
    poll_storage: String,
    complete_wrapper: String,
    complete_storage: String,
    panic_message_wrapper: String,
    panic_storage: String,
    cancel_wrapper: String,
    cancel_storage: String,
    free_wrapper: String,
    free_storage: String,
    params: Vec<argument::Conversion>,
    call_args: Vec<String>,
    complete_call_args: Vec<String>,
    returns: result::Conversion,
    fallible: Option<FallibleResult>,
}

pub struct Function {
    body: Body,
}

struct SyncFunction {
    pub python_name: String,
    pub wrapper: String,
    pub storage: String,
    pub params: Vec<argument::Conversion>,
    pub call_args: Vec<String>,
    pub returns: result::Conversion,
    mutation: Option<argument::MutationOutput>,
    fallible: Option<FallibleResult>,
    methods: Vec<ExtensionMethod>,
}

struct AsyncFunction {
    future_methods: NativeFutureMethods,
    start_wrapper: String,
    start_storage: String,
    poll_wrapper: String,
    poll_storage: String,
    complete_wrapper: String,
    complete_storage: String,
    panic_message_wrapper: String,
    panic_storage: String,
    cancel_wrapper: String,
    cancel_storage: String,
    free_wrapper: String,
    free_storage: String,
    params: Vec<argument::Conversion>,
    call_args: Vec<String>,
    complete_call_args: Vec<String>,
    returns: result::Conversion,
    fallible: Option<FallibleResult>,
    methods: Vec<ExtensionMethod>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeFutureMethods {
    start: String,
    poll: String,
    complete: String,
    cancel: String,
    free: String,
    panic_message: String,
}

impl NativeFutureMethods {
    pub fn new(start: impl Into<String>) -> Self {
        let start = start.into();
        Self {
            poll: Self::method(&start, "poll"),
            complete: Self::method(&start, "complete"),
            cancel: Self::method(&start, "cancel"),
            free: Self::method(&start, "free"),
            panic_message: Self::method(&start, "panic_message"),
            start,
        }
    }

    pub fn start(&self) -> &str {
        &self.start
    }

    pub fn poll(&self) -> &str {
        &self.poll
    }

    pub fn complete(&self) -> &str {
        &self.complete
    }

    pub fn cancel(&self) -> &str {
        &self.cancel
    }

    pub fn free(&self) -> &str {
        &self.free
    }

    pub fn panic_message(&self) -> &str {
        &self.panic_message
    }

    fn extension_methods(&self, methods: FutureExtensionMethods) -> Result<Vec<ExtensionMethod>> {
        [
            ExtensionMethod::new(self.start(), methods.start, MethodFlags::FastCall),
            ExtensionMethod::new(self.poll(), methods.poll, MethodFlags::FastCall),
            ExtensionMethod::new(self.complete(), methods.complete, MethodFlags::OneObject),
            ExtensionMethod::new(
                self.panic_message(),
                methods.panic_message,
                MethodFlags::OneObject,
            ),
            ExtensionMethod::new(self.cancel(), methods.cancel, MethodFlags::OneObject),
            ExtensionMethod::new(self.free(), methods.free, MethodFlags::OneObject),
        ]
        .into_iter()
        .collect()
    }

    fn method(start: &str, suffix: &'static str) -> String {
        format!("{start}__{suffix}")
    }
}

struct FutureExtensionMethods {
    start: String,
    poll: String,
    complete: String,
    cancel: String,
    free: String,
    panic_message: String,
}

impl FutureExtensionMethods {
    fn new(
        start: String,
        poll: String,
        complete: String,
        cancel: String,
        free: String,
        panic_message: String,
    ) -> Self {
        Self {
            start,
            poll,
            complete,
            cancel,
            free,
            panic_message,
        }
    }
}

enum Body {
    Sync(Box<SyncFunction>),
    Async(Box<AsyncFunction>),
    Skipped(SkippedFunction),
}

impl Function {
    pub fn from_declaration(
        declaration: &FunctionDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        Self::from_export(
            Name::new(declaration.name()).function(),
            declaration.symbol(),
            declaration.callable(),
            Vec::new(),
            bridge,
            context,
        )
    }

    pub fn from_export(
        python_name: String,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if let Some(unsupported) = UnsupportedCallable::from_parts(callable, &receiver_args) {
            return Ok(Self {
                body: Body::Skipped(SkippedFunction::new(python_name, unsupported)),
            });
        }
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => SyncFunction::from_export(
                python_name,
                symbol,
                callable,
                receiver_args,
                bridge,
                context,
            )
            .map(Box::new)
            .map(Body::Sync)
            .map(|body| Self { body }),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
                poll,
                complete,
                cancel,
                free,
                panic_message,
                ..
            }) => AsyncFunction::from_export(
                python_name,
                AsyncSymbols {
                    start: symbol,
                    poll,
                    complete,
                    cancel,
                    free,
                    panic_message,
                },
                callable,
                receiver_args,
                bridge,
                context,
            )
            .map(Box::new)
            .map(Body::Async)
            .map(|body| Self { body }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown function execution",
            }),
        }
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        self.body.methods().iter()
    }

    pub fn render(self) -> Result<Emitted> {
        match self.body {
            Body::Sync(function) => function.render(),
            Body::Async(function) => function.render(),
            Body::Skipped(function) => Ok(function.render()),
        }
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        match &self.body {
            Body::Sync(function) => function.primitives(),
            Body::Async(function) => function.primitives(),
            Body::Skipped(_) => Vec::new(),
        }
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> {
        match &self.body {
            Body::Sync(function) => function.wire_primitives().collect::<Vec<_>>(),
            Body::Async(function) => function.wire_primitives().collect::<Vec<_>>(),
            Body::Skipped(_) => Vec::new(),
        }
        .into_iter()
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> {
        match &self.body {
            Body::Sync(function) => function.direct_vector_elements().collect::<Vec<_>>(),
            Body::Async(function) => function.direct_vector_elements().collect::<Vec<_>>(),
            Body::Skipped(_) => Vec::new(),
        }
        .into_iter()
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.owned_buffers().next()
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> {
        match &self.body {
            Body::Sync(function) => function.owned_buffers().collect::<Vec<_>>(),
            Body::Async(function) => function.owned_buffers().collect::<Vec<_>>(),
            Body::Skipped(_) => Vec::new(),
        }
        .into_iter()
    }

    pub fn has_string_argument(&self) -> bool {
        match &self.body {
            Body::Sync(function) => function.has_string_argument(),
            Body::Async(function) => function.has_string_argument(),
            Body::Skipped(_) => false,
        }
    }

    pub fn has_bytes_argument(&self) -> bool {
        match &self.body {
            Body::Sync(function) => function.has_bytes_argument(),
            Body::Async(function) => function.has_bytes_argument(),
            Body::Skipped(_) => false,
        }
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        match &self.body {
            Body::Sync(function) => function.has_raw_wire_argument(),
            Body::Async(function) => function.has_raw_wire_argument(),
            Body::Skipped(_) => false,
        }
    }

    pub fn uses_async_protocol(&self) -> bool {
        matches!(self.body, Body::Async(_))
    }

    pub fn can_render(callable: &ExportedCallable<Native>) -> bool {
        UnsupportedCallable::from_callable(callable).is_none()
    }
}

impl Body {
    fn methods(&self) -> &[ExtensionMethod] {
        match self {
            Self::Sync(function) => &function.methods,
            Self::Async(function) => &function.methods,
            Self::Skipped(function) => &function.methods,
        }
    }
}

struct SkippedFunction {
    python_name: String,
    shape: &'static str,
    methods: Vec<ExtensionMethod>,
}

impl SkippedFunction {
    fn new(python_name: String, unsupported: UnsupportedCallable) -> Self {
        Self {
            python_name,
            shape: unsupported.shape(),
            methods: Vec::new(),
        }
    }

    fn render(self) -> Emitted {
        Emitted::diagnostic(Diagnostic::new(format!(
            "unsupported callable {}: {}",
            self.python_name, self.shape
        )))
    }
}

#[derive(Clone, Copy)]
struct UnsupportedCallable {
    shape: &'static str,
}

impl UnsupportedCallable {
    fn from_parts(
        callable: &ExportedCallable<Native>,
        receiver_args: &[argument::Conversion],
    ) -> Option<Self> {
        if matches!(callable.execution(), ExecutionDecl::Asynchronous(_))
            && receiver_args.iter().any(argument::Conversion::has_mutation)
        {
            return Some(Self {
                shape: "async mutable encoded receiver",
            });
        }

        Self::from_callable(callable)
    }

    fn from_callable(callable: &ExportedCallable<Native>) -> Option<Self> {
        callable
            .params()
            .iter()
            .find_map(|parameter| match parameter.payload() {
                IncomingParam::Value(plan) if plan.render_with(&mut MutableEncodedParameter) => {
                    Some(Self {
                        shape: "mutable encoded parameter",
                    })
                }
                IncomingParam::Value(_) | IncomingParam::Closure(_) => None,
            })
    }

    fn shape(self) -> &'static str {
        self.shape
    }
}

struct MutableEncodedParameter;

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for MutableEncodedParameter {
    type Output = bool;

    fn direct(&mut self, _: &TypeRef, _: Receive) -> Self::Output {
        false
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        _: &WritePlan,
        _: native::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        receive == Receive::ByMutRef
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        false
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        false
    }

    fn direct_vector(&mut self, _: &TypeRef) -> Self::Output {
        false
    }
}

impl SyncFunction {
    fn from_export(
        python_name: String,
        symbol: &NativeSymbol,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let loaded = bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "function without C bridge symbol",
            })?;
        let wrapper = format!("boltffi_python_callable_wrapper_{}", symbol.name().as_str());
        let method =
            ExtensionMethod::new(python_name.clone(), wrapper.clone(), MethodFlags::FastCall)?;
        let value_args = callable
            .params()
            .iter()
            .enumerate()
            .try_fold(
                (
                    receiver_args
                        .iter()
                        .map(argument::Conversion::c_arity)
                        .sum::<usize>(),
                    Vec::new(),
                ),
                |(c_offset, mut conversions), (offset, parameter)| {
                    let index = receiver_args.len() + offset;
                    let conversion = argument::Conversion::from_parameter(
                        symbol.name().as_str(),
                        index,
                        parameter,
                        &loaded.function().params()[c_offset..],
                        bridge,
                        context,
                    )?;
                    let c_offset = c_offset + conversion.c_arity();
                    conversions.push(conversion);
                    Ok::<_, Error>((c_offset, conversions))
                },
            )?
            .1;
        let params = receiver_args
            .into_iter()
            .chain(value_args)
            .collect::<Vec<_>>();
        let base_call_args = params
            .iter()
            .flat_map(argument::Conversion::call_args)
            .collect::<Vec<_>>();
        let fallible = FallibleResult::new(callable, loaded, base_call_args.len())?;
        let call_args = base_call_args
            .into_iter()
            .chain(fallible.iter().filter_map(FallibleResult::success_argument))
            .collect();
        let returns = result::Conversion::from_plan(callable.returns().plan(), bridge, context)?;
        let mutation = Self::mutation(&params, &returns, fallible.is_some())?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().to_owned(),
            params,
            call_args,
            returns,
            mutation,
            fallible,
            methods: vec![method],
        })
    }

    fn render(self) -> Result<Emitted> {
        let source = SyncTemplate {
            python_name: self.python_name,
            wrapper: self.wrapper,
            storage: self.storage,
            params: self.params,
            call_args: self.call_args,
            returns: self.returns,
            mutation: self.mutation,
            fallible: self.fallible,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    fn mutation(
        params: &[argument::Conversion],
        returns: &result::Conversion,
        fallible: bool,
    ) -> Result<Option<argument::MutationOutput>> {
        let mut mutations = params.iter().filter_map(argument::Conversion::mutation);
        let mutation = mutations.next();
        if mutations.next().is_some() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "multiple mutable encoded parameters",
            });
        }
        if mutation.is_some() && (fallible || !returns.is_void()) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "mutable encoded parameter with non-void return",
            });
        }
        Ok(mutation)
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        let params = self
            .params
            .iter()
            .filter_map(argument::Conversion::primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_primitives),
            )
            .collect::<Vec<_>>();
        params
            .into_iter()
            .chain(self.returns.primitive())
            .chain(self.fallible.iter().filter_map(FallibleResult::primitive))
            .collect()
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::wire_primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_wire_primitives),
            )
    }

    fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::direct_vector_element)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_direct_vector_elements),
            )
            .chain(self.returns.direct_vector_element())
            .chain(
                self.fallible
                    .iter()
                    .filter_map(FallibleResult::direct_vector_element),
            )
    }

    fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.returns.owned_buffer().into_iter().chain(
            self.fallible
                .iter()
                .filter_map(FallibleResult::owned_buffer),
        )
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_string)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_string_argument())
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_bytes)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_bytes_argument())
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_raw_wire)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_raw_wire_argument())
    }
}

#[derive(Clone, Copy)]
struct AsyncSymbols<'symbol> {
    start: &'symbol NativeSymbol,
    poll: &'symbol NativeSymbol,
    complete: &'symbol NativeSymbol,
    cancel: &'symbol NativeSymbol,
    free: &'symbol NativeSymbol,
    panic_message: &'symbol NativeSymbol,
}

impl AsyncFunction {
    fn from_export(
        python_name: String,
        symbols: AsyncSymbols<'_>,
        callable: &ExportedCallable<Native>,
        receiver_args: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let start = Self::loaded(symbols.start, bridge, "async start symbol")?;
        let poll = Self::loaded(symbols.poll, bridge, "async poll symbol")?;
        let complete = Self::loaded(symbols.complete, bridge, "async complete symbol")?;
        let cancel = Self::loaded(symbols.cancel, bridge, "async cancel symbol")?;
        let free = Self::loaded(symbols.free, bridge, "async free symbol")?;
        let panic_message = Self::loaded(symbols.panic_message, bridge, "async panic symbol")?;
        let future_methods = NativeFutureMethods::new(python_name);
        let start_wrapper = Self::wrapper(symbols.start);
        let poll_wrapper = Self::wrapper(symbols.poll);
        let complete_wrapper = Self::wrapper(symbols.complete);
        let panic_message_wrapper = Self::wrapper(symbols.panic_message);
        let cancel_wrapper = Self::wrapper(symbols.cancel);
        let free_wrapper = Self::wrapper(symbols.free);
        let value_args = callable
            .params()
            .iter()
            .enumerate()
            .try_fold(
                (
                    receiver_args
                        .iter()
                        .map(argument::Conversion::c_arity)
                        .sum::<usize>(),
                    Vec::new(),
                ),
                |(c_offset, mut conversions), (offset, parameter)| {
                    let index = receiver_args.len() + offset;
                    let conversion = argument::Conversion::from_parameter(
                        symbols.start.name().as_str(),
                        index,
                        parameter,
                        &start.function().params()[c_offset..],
                        bridge,
                        context,
                    )?;
                    let c_offset = c_offset + conversion.c_arity();
                    conversions.push(conversion);
                    Ok::<_, Error>((c_offset, conversions))
                },
            )?
            .1;
        let params = receiver_args
            .into_iter()
            .chain(value_args)
            .collect::<Vec<_>>();
        let call_args = params
            .iter()
            .flat_map(argument::Conversion::call_args)
            .collect::<Vec<_>>();
        let fallible = FallibleResult::new(callable, complete, 2)?;
        let complete_call_args = fallible
            .iter()
            .filter_map(FallibleResult::success_argument)
            .collect::<Vec<_>>();
        let returns = result::Conversion::from_plan(callable.returns().plan(), bridge, context)?;
        let methods = future_methods.extension_methods(FutureExtensionMethods::new(
            start_wrapper.clone(),
            poll_wrapper.clone(),
            complete_wrapper.clone(),
            cancel_wrapper.clone(),
            free_wrapper.clone(),
            panic_message_wrapper.clone(),
        ))?;
        Ok(Self {
            future_methods,
            start_wrapper,
            start_storage: start.storage_name().to_owned(),
            poll_wrapper,
            poll_storage: poll.storage_name().to_owned(),
            complete_wrapper,
            complete_storage: complete.storage_name().to_owned(),
            panic_message_wrapper,
            panic_storage: panic_message.storage_name().to_owned(),
            cancel_wrapper,
            cancel_storage: cancel.storage_name().to_owned(),
            free_wrapper,
            free_storage: free.storage_name().to_owned(),
            params,
            call_args,
            complete_call_args,
            returns,
            fallible,
            methods,
        })
    }

    fn render(self) -> Result<Emitted> {
        let source = AsyncTemplate {
            python_name: self.future_methods.start,
            start_wrapper: self.start_wrapper,
            start_storage: self.start_storage,
            poll_python_name: self.future_methods.poll,
            poll_wrapper: self.poll_wrapper,
            poll_storage: self.poll_storage,
            complete_wrapper: self.complete_wrapper,
            complete_storage: self.complete_storage,
            panic_message_wrapper: self.panic_message_wrapper,
            panic_storage: self.panic_storage,
            cancel_wrapper: self.cancel_wrapper,
            cancel_storage: self.cancel_storage,
            free_wrapper: self.free_wrapper,
            free_storage: self.free_storage,
            params: self.params,
            call_args: self.call_args,
            complete_call_args: self.complete_call_args,
            returns: self.returns,
            fallible: self.fallible,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        self.params
            .iter()
            .filter_map(argument::Conversion::primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_primitives),
            )
            .chain(self.returns.primitive())
            .chain(self.fallible.iter().filter_map(FallibleResult::primitive))
            .collect()
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::wire_primitive)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_wire_primitives),
            )
    }

    fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(argument::Conversion::direct_vector_element)
            .chain(
                self.params
                    .iter()
                    .flat_map(argument::Conversion::closure_direct_vector_elements),
            )
            .chain(self.returns.direct_vector_element())
            .chain(
                self.fallible
                    .iter()
                    .filter_map(FallibleResult::direct_vector_element),
            )
    }

    fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.returns.owned_buffer().into_iter().chain(
            self.fallible
                .iter()
                .filter_map(FallibleResult::owned_buffer),
        )
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_string)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_string_argument())
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_bytes)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_bytes_argument())
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(argument::Conversion::is_raw_wire)
            || self
                .params
                .iter()
                .any(|param| param.has_closure_raw_wire_argument())
    }

    fn loaded<'bridge>(
        symbol: &NativeSymbol,
        bridge: &'bridge PythonCExtBridgeContract,
        shape: &'static str,
    ) -> Result<&'bridge LoadedFunction> {
        bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape,
            })
    }

    fn wrapper(symbol: &NativeSymbol) -> String {
        format!("boltffi_python_callable_wrapper_{}", symbol.name().as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleResult {
    success_declaration: Option<String>,
    success_argument: Option<String>,
    success_value: String,
    error_type: String,
    error_value: String,
    error: result::Conversion,
}

impl FallibleResult {
    fn new(
        callable: &ExportedCallable<Native>,
        loaded: &LoadedFunction,
        argument_count: usize,
    ) -> Result<Option<Self>> {
        match callable.error() {
            ErrorDecl::None(_) => Ok(None),
            ErrorDecl::EncodedViaReturnSlot {
                shape: native::BufferShape::Buffer,
                ..
            } => Self::encoded(callable.returns().plan(), loaded, argument_count).map(Some),
            ErrorDecl::EncodedViaReturnSlot { .. } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error buffer shape",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible function",
            }),
        }
    }

    fn success_argument(&self) -> Option<String> {
        self.success_argument.clone()
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.error.primitive()
    }

    fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        self.error.direct_vector_element()
    }

    fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.error.owned_buffer()
    }

    fn encoded(
        success: &ReturnPlan<Native, OutOfRust>,
        loaded: &LoadedFunction,
        argument_count: usize,
    ) -> Result<Self> {
        if !matches!(loaded.function().returns(), c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error return",
            });
        }
        let error = result::Conversion::from_owned_buffer(result::OwnedBuffer::RawWire)?;
        let success = FallibleSuccess::new(loaded.function(), argument_count).render(success)?;
        Ok(Self {
            success_declaration: success.declaration,
            success_argument: success.argument,
            success_value: success.value,
            error_type: TypeSyntax::new(loaded.function().returns()).anonymous()?,
            error_value: "return_error".to_owned(),
            error,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleSuccessBinding {
    declaration: Option<String>,
    argument: Option<String>,
    value: String,
}

impl FallibleSuccessBinding {
    fn empty() -> Self {
        Self {
            declaration: None,
            argument: None,
            value: String::new(),
        }
    }

    fn out_pointer(ty: &c::Type) -> Result<Self> {
        let value = "return_success".to_owned();
        Ok(Self {
            declaration: Some(TypeSyntax::new(ty).declaration(&value)?),
            argument: Some(format!("&{value}")),
            value,
        })
    }
}

struct FallibleSuccess<'function> {
    function: &'function c::Function,
    argument_count: usize,
}

impl<'function> FallibleSuccess<'function> {
    fn new(function: &'function c::Function, argument_count: usize) -> Self {
        Self {
            function,
            argument_count,
        }
    }

    fn render(mut self, plan: &ReturnPlan<Native, OutOfRust>) -> Result<FallibleSuccessBinding> {
        plan.render_with(&mut self)
    }

    fn out_pointer(&self, slot: ReturnValueSlot) -> Result<FallibleSuccessBinding> {
        match slot {
            ReturnValueSlot::OutPointer => {
                let parameter = self.function.params().get(self.argument_count).ok_or(
                    Error::UnsupportedTarget {
                        target: "python",
                        shape: "missing fallible success out parameter",
                    },
                )?;
                let c::Type::MutPointer(success_type) = parameter.ty() else {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "fallible success parameter",
                    });
                };
                FallibleSuccessBinding::out_pointer(success_type.as_ref())
            }
            ReturnValueSlot::ReturnSlot => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible success return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible success return",
            }),
        }
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for FallibleSuccess<'_> {
    type Output = Result<FallibleSuccessBinding>;

    fn void(&mut self) -> Self::Output {
        Ok(FallibleSuccessBinding::empty())
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &TypeRef) -> Self::Output {
        self.out_pointer(slot)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &TypeRef,
        _: &ReadPlan,
        _: native::BufferShape,
    ) -> Self::Output {
        self.out_pointer(slot)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        self.out_pointer(slot)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible success return",
        })
    }

    fn direct_vector(&mut self, _: &TypeRef) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible success return",
        })
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "fallible success return",
        })
    }
}

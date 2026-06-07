use boltffi_ast::{FnSig, ReturnDef, TypeExpr};
use boltffi_binding::{
    ClosureForm, ClosureReturn, ErrorDecl, HandlePresence, IncomingParam, Native, OutOfRust,
    ParamPlan, ReadPlan, Receive, ReturnPlan, TypeRef, Wasm32, WritePlan, native, wasm32,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, callable::signature, codec, local},
    target::Target,
};

use super::{RustInvocation, Tokens, encoded};

pub struct Rule;
pub struct Write;

pub struct Input<'a, S: Target> {
    closure: &'a ClosureReturn<S, OutOfRust>,
    source: &'a FnSig,
    rust_type: Option<Type>,
    invocation: RustInvocation,
}

pub struct WriteInput<'a, S: Target> {
    closure: &'a ClosureReturn<S, OutOfRust>,
    source: &'a FnSig,
    rust_type: Type,
    value: Ident,
    owner: Ident,
    lane: ReturnLane,
    span: Span,
}

#[derive(Clone, Copy)]
enum ReturnLane {
    Return,
    Success,
}

pub struct WriteTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    body: TokenStream,
}

impl<'a, S: Target> Input<'a, S> {
    pub fn new(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: &'a FnSig,
        rust_type: Option<Type>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            closure,
            source,
            rust_type,
            invocation,
        }
    }
}

impl<'a, S: Target> WriteInput<'a, S> {
    pub fn returned(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: &'a FnSig,
        rust_type: Type,
        value: Ident,
        owner: Ident,
    ) -> Self {
        Self::new(closure, source, rust_type, value, owner, ReturnLane::Return)
    }

    pub fn success(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: &'a FnSig,
        rust_type: Type,
        value: Ident,
        owner: Ident,
    ) -> Self {
        Self::new(
            closure,
            source,
            rust_type,
            value,
            owner,
            ReturnLane::Success,
        )
    }

    fn new(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: &'a FnSig,
        rust_type: Type,
        value: Ident,
        owner: Ident,
        lane: ReturnLane,
    ) -> Self {
        let span = owner.span();
        Self {
            closure,
            source,
            rust_type,
            value,
            owner,
            lane,
            span,
        }
    }
}

impl ReturnLane {
    fn suffix(self) -> &'static str {
        match self {
            Self::Return => "closure",
            Self::Success => "success_closure",
        }
    }
}

impl WriteTokens {
    pub fn into_parts(self) -> (Vec<TokenStream>, Vec<TokenStream>, TokenStream) {
        (self.items, self.ffi_parameters, self.body)
    }
}

impl<'a, S> RenderRule<S, Input<'a, S>> for Rule
where
    S: Target,
    Write: RenderRule<S, WriteInput<'a, S>, Output = WriteTokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'a, S>) -> Result<Self::Output, Error> {
        let rust_type = input.rust_type.ok_or(Error::SourceSyntaxMismatch(
            "closure return requires a source return type",
        ))?;
        let RustInvocation {
            function,
            conversions,
            writebacks,
            arguments,
        } = input.invocation;
        let value = local::Wrapper::new(function.span()).closure();
        let writer = <Write as RenderRule<S, _>>::apply(
            Write,
            WriteInput::returned(
                input.closure,
                input.source,
                rust_type,
                value.clone(),
                function.clone(),
            ),
        )?;
        let (items, ffi_parameters, body) = writer.into_parts();

        Ok(Tokens {
            items,
            ffi_parameters,
            return_type: quote! { -> ::boltffi::__private::FfiStatus },
            body: quote! {
                #(#conversions)*
                let #value = #function(#(#arguments),*);
                #(#writebacks)*
                #body
                ::boltffi::__private::FfiStatus::OK
            },
        })
    }
}

impl<'a> RenderRule<Native, WriteInput<'a, Native>> for Write {
    type Output = WriteTokens;

    fn apply(self, input: WriteInput<'a, Native>) -> Result<Self::Output, Error> {
        match input.closure.registration().shape() {
            native::ClosureRegistration::InvokeContextRelease => NativeClosure::new(input).tokens(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure return registration",
            )),
        }
    }
}

impl<'a> RenderRule<Wasm32, WriteInput<'a, Wasm32>> for Write {
    type Output = WriteTokens;

    fn apply(self, input: WriteInput<'a, Wasm32>) -> Result<Self::Output, Error> {
        WasmClosure::new(input).tokens()
    }
}

struct NativeClosure<'a> {
    input: WriteInput<'a, Native>,
}

impl<'a> NativeClosure<'a> {
    fn new(input: WriteInput<'a, Native>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<WriteTokens, Error> {
        let returned_closure = ReturnedClosure::new(
            self.input.source,
            self.input.closure,
            Some(&self.input.rust_type),
        )?;
        let invoke = ClosureInvoke::<Native>::new(
            self.input.closure.invoke(),
            self.input.source,
            &returned_closure,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let invoke_parameters = invoke.parameters(&failure)?;
        let parameter_items = invoke_parameters.items;
        let return_ffi_parameters = return_tokens.ffi_parameters();
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let storage = format_ident!("__BoltffiClosureReturn{}", self.input.value);
        let lane = self.input.lane.suffix();
        let call = format_ident!("__boltffi_{}_{}_call", self.input.owner, lane);
        let release = format_ident!("__boltffi_{}_{}_release", self.input.owner, lane);
        let locals = local::Wrapper::new(self.input.span);
        let output = locals.return_out();
        let context = locals.closure_context();
        let ffi_parameter_types = invoke_parameters
            .ffi_parameter_types
            .into_iter()
            .chain(return_ffi_parameter_types)
            .collect::<Vec<_>>();
        let ffi_parameters = invoke_parameters
            .ffi_parameters
            .into_iter()
            .chain(return_ffi_parameters)
            .collect::<Vec<_>>();
        let conversions = invoke_parameters.conversions;
        let arguments = invoke_parameters.arguments;
        let return_type = return_tokens.return_type();
        let invocation = returned_closure.invocation();
        let call_body = return_tokens.body(quote! { #invocation(#(#arguments),*) });
        let context_type = returned_closure.context_type();
        let context_binding = returned_closure.context_binding(quote! {
            __boltffi_context as *mut #context_type
        });
        let context_value = returned_closure.context_value(&self.input.value)?;
        let write_present = quote! {
            let #context = Box::into_raw(Box::new(#context_value)) as *mut ::core::ffi::c_void;
            unsafe {
                *#output = #storage {
                    invoke: Some(#call),
                    context: #context,
                    release: Some(#release),
                };
            }
        };
        let write_body = returned_closure.write_body(
            &self.input.value,
            write_present,
            quote! {
                unsafe {
                    *#output = #storage {
                        invoke: None,
                        context: ::core::ptr::null_mut(),
                        release: None,
                    };
                }
            },
        )?;

        let items = parameter_items
            .into_iter()
            .chain([quote! {
                #[cfg(not(target_arch = "wasm32"))]
                unsafe extern "C" fn #call(
                    __boltffi_context: *mut ::core::ffi::c_void,
                    #(#ffi_parameters),*
                ) #return_type {
                    let mut __boltffi_closure = unsafe { #context_binding };
                    #(#conversions)*
                    #call_body
                }

                #[cfg(not(target_arch = "wasm32"))]
                unsafe extern "C" fn #release(__boltffi_context: *mut ::core::ffi::c_void) {
                    if !__boltffi_context.is_null() {
                        unsafe {
                            drop(Box::from_raw(__boltffi_context as *mut #context_type));
                        }
                    }
                }
            }])
            .collect();

        Ok(WriteTokens {
            items,
            ffi_parameters: vec![quote! { #output: *mut ::core::ffi::c_void }],
            body: quote! {
                #[repr(C)]
                struct #storage {
                    invoke: Option<unsafe extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type>,
                    context: *mut ::core::ffi::c_void,
                    release: Option<unsafe extern "C" fn(*mut ::core::ffi::c_void)>,
                }

                if #output.is_null() {
                    ::boltffi::__private::set_last_error("closure return out pointer is null".to_string());
                    return ::boltffi::__private::FfiStatus::INVALID_ARG;
                }
                let #output = #output as *mut #storage;
                #write_body
            },
        })
    }
}

struct WasmClosure<'a> {
    input: WriteInput<'a, Wasm32>,
}

impl<'a> WasmClosure<'a> {
    fn new(input: WriteInput<'a, Wasm32>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<WriteTokens, Error> {
        let returned_closure = ReturnedClosure::new(
            self.input.source,
            self.input.closure,
            Some(&self.input.rust_type),
        )?;
        let invoke = ClosureInvoke::<Wasm32>::new(
            self.input.closure.invoke(),
            self.input.source,
            &returned_closure,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let invoke_parameters = invoke.parameters(&failure)?;
        let parameter_items = invoke_parameters.items;
        let return_ffi_parameters = return_tokens.ffi_parameters();
        let registration = self.input.closure.registration().shape();
        let call = Ident::new(registration.call().name().as_str(), self.input.span);
        let release = Ident::new(registration.free().name().as_str(), self.input.span);
        let output = local::Wrapper::new(self.input.span).return_out();
        let ffi_parameters = invoke_parameters
            .ffi_parameters
            .into_iter()
            .chain(return_ffi_parameters)
            .collect::<Vec<_>>();
        let conversions = invoke_parameters.conversions;
        let arguments = invoke_parameters.arguments;
        let return_type = return_tokens.return_type();
        let invocation = returned_closure.invocation();
        let call_body = return_tokens.body(quote! { #invocation(#(#arguments),*) });
        let context_type = returned_closure.context_type();
        let context_binding = returned_closure.context_binding(quote! {
            __boltffi_context as usize as *mut #context_type
        });
        let context_value = returned_closure.context_value(&self.input.value)?;
        let write_present = quote! {
            unsafe {
                *#output = Box::into_raw(Box::new(#context_value)) as usize as u32;
            }
        };
        let write_body = returned_closure.write_body(
            &self.input.value,
            write_present,
            quote! {
                unsafe {
                    *#output = 0;
                }
            },
        )?;

        let items = parameter_items
            .into_iter()
            .chain([quote! {
                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #call(
                    __boltffi_context: u32,
                    #(#ffi_parameters),*
                ) #return_type {
                    let mut __boltffi_closure = unsafe { #context_binding };
                    #(#conversions)*
                    #call_body
                }

                #[cfg(target_arch = "wasm32")]
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #release(__boltffi_context: u32) {
                    if __boltffi_context != 0 {
                        unsafe {
                            drop(Box::from_raw(__boltffi_context as usize as *mut #context_type));
                        }
                    }
                }
            }])
            .collect();

        Ok(WriteTokens {
            items,
            ffi_parameters: vec![quote! { #output: *mut u32 }],
            body: quote! {
                if #output.is_null() {
                    ::boltffi::__private::set_last_error("closure return out pointer is null".to_string());
                    return ::boltffi::__private::FfiStatus::INVALID_ARG;
                }
                #write_body
            },
        })
    }
}

struct ClosureInvoke<'a, S: Target> {
    callable: &'a boltffi_binding::ExportedCallable<S>,
    source: &'a FnSig,
    returned_closure: &'a ReturnedClosure,
}

impl<'a, S: Target> ClosureInvoke<'a, S> {
    fn new(
        callable: &'a boltffi_binding::ExportedCallable<S>,
        source: &'a FnSig,
        returned_closure: &'a ReturnedClosure,
    ) -> Result<Self, Error> {
        if callable.params().len() != source.parameters.len() {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter count does not match binding invoke parameter count",
            ));
        }
        Ok(Self {
            callable,
            source,
            returned_closure,
        })
    }

    fn parameters(&self, failure: &TokenStream) -> Result<InvokeParameters, Error>
    where
        InvokeParameterRule:
            RenderRule<S, InvokeParameterInput<'a, S>, Output = InvokeParameterTokens>,
    {
        self.callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .zip(self.returned_closure.signature.parameters.iter())
            .enumerate()
            .map(|(index, ((param, source), rust_type))| {
                <InvokeParameterRule as RenderRule<S, _>>::apply(
                    InvokeParameterRule,
                    InvokeParameterInput {
                        index,
                        payload: param.payload(),
                        source,
                        rust_type,
                        failure: failure.clone(),
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .map(InvokeParameters::from)
    }

    fn return_tokens(&self) -> Result<RustClosureReturnTokens, Error>
    where
        RustClosureReturnRule:
            RenderRule<S, RustClosureReturn<'a, S>, Output = RustClosureReturnTokens>,
    {
        <RustClosureReturnRule as RenderRule<S, _>>::apply(
            RustClosureReturnRule,
            RustClosureReturn::new(
                self.callable.returns().plan(),
                self.callable.error(),
                &self.source.returns,
                self.returned_closure.signature.return_type.as_ref(),
            ),
        )
    }
}

struct InvokeParameterRule;

struct InvokeParameterInput<'a, S: Target> {
    index: usize,
    payload: &'a IncomingParam<S>,
    source: &'a TypeExpr,
    rust_type: &'a Type,
    failure: TokenStream,
}

impl<'a, S: Target> InvokeParameterInput<'a, S> {
    fn direct_tokens(&self) -> Result<Option<InvokeParameterTokens>, Error>
    where
        for<'ty> render::type_ref::Rule: RenderRule<S, &'ty TypeRef, Output = TokenStream>,
        for<'binding> render::param::closure::Rule: RenderRule<
                S,
                render::param::closure::Input<'binding, S>,
                Output = render::param::Tokens,
            >,
    {
        let argument = local::ClosureArgument::new(self.index).value();
        match self.payload {
            IncomingParam::Value(ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                ..
            }) => {
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?;
                Ok(Some(InvokeParameterTokens::single(
                    quote! { #argument: #ffi_type },
                    ffi_type,
                    TokenStream::new(),
                    quote! { #argument },
                )))
            }
            IncomingParam::Value(ParamPlan::Direct { .. }) => Ok(Some({
                let rust_type = self.rust_type;
                InvokeParameterTokens::single(
                    quote! {
                        #argument: <#rust_type as ::boltffi::__private::Passable>::In
                    },
                    quote! {
                        <#rust_type as ::boltffi::__private::Passable>::In
                    },
                    quote! {
                        let #argument: #rust_type = unsafe {
                            <#rust_type as ::boltffi::__private::Passable>::unpack(#argument)
                        };
                    },
                    quote! { #argument },
                )
            })),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Ok(None),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion(
                "closure return invoke parameter shape",
            )),
            IncomingParam::Closure(closure) => {
                let signature = signature::closure_signature(self.source, closure.presence())?;
                let tokens = <render::param::closure::Rule as RenderRule<S, _>>::apply(
                    render::param::closure::Rule,
                    render::param::closure::Input::new(
                        closure,
                        signature,
                        self.rust_type.clone(),
                        argument.clone(),
                        self.failure.clone(),
                    ),
                )?;
                let conversions = tokens.conversions();
                Ok(Some(InvokeParameterTokens {
                    items: tokens.items().to_vec(),
                    ffi_parameters: tokens.ffi_parameters().to_vec(),
                    ffi_parameter_types: tokens.ffi_parameter_types().to_vec(),
                    conversion: quote! { #(#conversions)* },
                    argument: tokens.argument().clone(),
                }))
            }
        }
    }

    fn encoded_tokens(
        &self,
        codec: &WritePlan,
        receive: Receive,
    ) -> Result<InvokeParameterTokens, Error> {
        let locals = local::ClosureArgument::new(self.index);
        let argument = locals.value();
        let pointer = locals.pointer();
        let length = locals.length();
        let conversion =
            codec::EncodedValue::new(codec.root()).conversion(codec::DecodeInput::new(
                receive,
                self.rust_type,
                &argument,
                &pointer,
                &length,
                &self.failure,
            ))?;

        Ok(InvokeParameterTokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #pointer: *const u8 }, quote! { #length: usize }],
            ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
            conversion,
            argument: quote! { #argument },
        })
    }
}

impl<'a> RenderRule<Native, InvokeParameterInput<'a, Native>> for InvokeParameterRule {
    type Output = InvokeParameterTokens;

    fn apply(self, input: InvokeParameterInput<'a, Native>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match input.payload {
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                receive,
                shape: native::BufferShape::Slice,
                ..
            }) => input.encoded_tokens(codec, *receive),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedExpansion(
                "native closure return invoke encoded parameter shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "closure return invoke parameter shape",
            )),
        }
    }
}

impl<'a> RenderRule<Wasm32, InvokeParameterInput<'a, Wasm32>> for InvokeParameterRule {
    type Output = InvokeParameterTokens;

    fn apply(self, input: InvokeParameterInput<'a, Wasm32>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match input.payload {
            IncomingParam::Value(ParamPlan::Encoded {
                codec,
                receive,
                shape: wasm32::BufferShape::Slice,
                ..
            }) => input.encoded_tokens(codec, *receive),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedExpansion(
                "wasm closure return invoke encoded parameter shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "closure return invoke parameter shape",
            )),
        }
    }
}

struct InvokeParameterTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversion: TokenStream,
    argument: TokenStream,
}

impl InvokeParameterTokens {
    fn single(
        ffi_parameter: TokenStream,
        ffi_parameter_type: TokenStream,
        conversion: TokenStream,
        argument: TokenStream,
    ) -> Self {
        Self {
            items: Vec::new(),
            ffi_parameters: vec![ffi_parameter],
            ffi_parameter_types: vec![ffi_parameter_type],
            conversion,
            argument,
        }
    }
}

struct InvokeParameters {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    conversions: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

impl From<Vec<InvokeParameterTokens>> for InvokeParameters {
    fn from(tokens: Vec<InvokeParameterTokens>) -> Self {
        Self {
            items: tokens
                .iter()
                .flat_map(|token| token.items.iter().cloned())
                .collect(),
            ffi_parameters: tokens
                .iter()
                .flat_map(|token| token.ffi_parameters.iter().cloned())
                .collect(),
            ffi_parameter_types: tokens
                .iter()
                .flat_map(|token| token.ffi_parameter_types.iter().cloned())
                .collect(),
            conversions: tokens
                .iter()
                .map(|token| token.conversion.clone())
                .collect(),
            arguments: tokens.iter().map(|token| token.argument.clone()).collect(),
        }
    }
}

struct RustClosureReturnRule;

struct RustClosureReturn<'a, S: Target> {
    plan: &'a ReturnPlan<S, OutOfRust>,
    error: &'a ErrorDecl<S, OutOfRust>,
    source: &'a ReturnDef,
    rust_type: Option<&'a Type>,
}

impl<'a, S: Target> RustClosureReturn<'a, S> {
    fn new(
        plan: &'a ReturnPlan<S, OutOfRust>,
        error: &'a ErrorDecl<S, OutOfRust>,
        source: &'a ReturnDef,
        rust_type: Option<&'a Type>,
    ) -> Self {
        Self {
            plan,
            error,
            source,
            rust_type,
        }
    }

    fn direct_tokens<T: Target>(&self) -> Result<Option<RustClosureReturnTokens>, Error>
    where
        for<'ty> render::type_ref::Rule: RenderRule<T, &'ty TypeRef, Output = TokenStream>,
    {
        if !matches!(self.error, ErrorDecl::None(_)) {
            return Ok(None);
        }

        match self.plan {
            ReturnPlan::Void => {
                if !matches!(self.source, ReturnDef::Void) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                Ok(Some(RustClosureReturnTokens::Void))
            }
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <render::type_ref::Rule as RenderRule<T, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?;
                Ok(Some(RustClosureReturnTokens::DirectPrimitive { ffi_type }))
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let rust_type = self.rust_type.ok_or(Error::SourceSyntaxMismatch(
                    "closure return invoke direct return requires source return type",
                ))?;
                Ok(Some(RustClosureReturnTokens::DirectPassable {
                    rust_type: Box::new(rust_type.clone()),
                }))
            }
            ReturnPlan::EncodedViaReturnSlot { .. } => Ok(None),
            _ => Err(Error::UnsupportedExpansion(
                "closure return invoke return shape",
            )),
        }
    }

    fn rust_fallible_return(&self) -> Result<RustFallibleReturn, Error> {
        let ok = signature::Return::new(self.source)
            .fallible()?
            .ok_written_type()?;
        Ok(RustFallibleReturn { ok })
    }

    fn encoded_error<T: Target>(
        &self,
        error_codec: &'a ReadPlan,
        error_shape: T::BufferShape,
    ) -> Result<EncodedError, Error>
    where
        encoded::Rule: RenderRule<T, encoded::Input<'a, T>, Output = encoded::Tokens>
            + RenderRule<T, encoded::Empty<T>, Output = encoded::Tokens>,
    {
        let error_ident = local::Wrapper::new(Span::call_site()).error();
        let error = <encoded::Rule as RenderRule<T, _>>::apply(
            encoded::Rule,
            encoded::Input::new(error_codec, error_shape, error_ident),
        )?;
        let empty = <encoded::Rule as RenderRule<T, _>>::apply(
            encoded::Rule,
            encoded::Empty::new(error_shape),
        )?;

        Ok(EncodedError {
            return_type: error.return_type().clone(),
            value: error.value().clone(),
            empty_value: empty.value().clone(),
        })
    }
}

impl<'a> RenderRule<Native, RustClosureReturn<'a, Native>> for RustClosureReturnRule {
    type Output = RustClosureReturnTokens;

    fn apply(self, input: RustClosureReturn<'a, Native>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens::<Native>()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (
                ReturnPlan::EncodedViaReturnSlot {
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::None(_),
            ) => Ok(RustClosureReturnTokens::NativeEncoded),
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => Ok(RustClosureReturnTokens::fallible(
                input.encoded_error::<Native>(codec, native::BufferShape::Buffer)?,
                FallibleSuccess::Void,
            )),
            (
                ReturnPlan::DirectViaOutPointer {
                    ty: TypeRef::Primitive(primitive),
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let ffi_type = <render::type_ref::Rule as RenderRule<Native, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &TypeRef::Primitive(*primitive),
                )?;
                Ok(RustClosureReturnTokens::fallible(
                    input.encoded_error::<Native>(codec, native::BufferShape::Buffer)?,
                    FallibleSuccess::DirectPrimitive { ffi_type },
                ))
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => Ok(RustClosureReturnTokens::fallible(
                input.encoded_error::<Native>(codec, native::BufferShape::Buffer)?,
                FallibleSuccess::DirectPassable {
                    rust_type: Box::new(input.rust_fallible_return()?.ok),
                },
            )),
            (
                ReturnPlan::EncodedViaOutPointer {
                    codec: ok_codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec: error_codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let success_ident = local::Wrapper::new(Span::call_site()).success();
                let success = <encoded::Rule as RenderRule<Native, _>>::apply(
                    encoded::Rule,
                    encoded::Input::new(ok_codec, native::BufferShape::Buffer, success_ident),
                )?;
                Ok(RustClosureReturnTokens::fallible(
                    input.encoded_error::<Native>(error_codec, native::BufferShape::Buffer)?,
                    FallibleSuccess::Encoded {
                        out_type: success.return_type_without_arrow(),
                        value: success.value().clone(),
                    },
                ))
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "native closure return invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "closure return invoke return shape",
            )),
        }
    }
}

impl<'a> RenderRule<Wasm32, RustClosureReturn<'a, Wasm32>> for RustClosureReturnRule {
    type Output = RustClosureReturnTokens;

    fn apply(self, input: RustClosureReturn<'a, Wasm32>) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens::<Wasm32>()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (
                ReturnPlan::EncodedViaReturnSlot {
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::None(_),
            ) => Ok(RustClosureReturnTokens::WasmEncoded),
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(RustClosureReturnTokens::fallible(
                input.encoded_error::<Wasm32>(codec, wasm32::BufferShape::Packed)?,
                FallibleSuccess::Void,
            )),
            (
                ReturnPlan::DirectViaOutPointer {
                    ty: TypeRef::Primitive(primitive),
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let ffi_type = <render::type_ref::Rule as RenderRule<Wasm32, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &TypeRef::Primitive(*primitive),
                )?;
                Ok(RustClosureReturnTokens::fallible(
                    input.encoded_error::<Wasm32>(codec, wasm32::BufferShape::Packed)?,
                    FallibleSuccess::DirectPrimitive { ffi_type },
                ))
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(RustClosureReturnTokens::fallible(
                input.encoded_error::<Wasm32>(codec, wasm32::BufferShape::Packed)?,
                FallibleSuccess::DirectPassable {
                    rust_type: Box::new(input.rust_fallible_return()?.ok),
                },
            )),
            (
                ReturnPlan::EncodedViaOutPointer {
                    codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    codec: error_codec,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let success_ident = local::Wrapper::new(Span::call_site()).success();
                let success = <encoded::Rule as RenderRule<Wasm32, _>>::apply(
                    encoded::Rule,
                    encoded::Input::new(codec, wasm32::BufferShape::Packed, success_ident),
                )?;
                Ok(RustClosureReturnTokens::fallible(
                    input.encoded_error::<Wasm32>(error_codec, wasm32::BufferShape::Packed)?,
                    FallibleSuccess::Encoded {
                        out_type: success.return_type_without_arrow(),
                        value: success.value().clone(),
                    },
                ))
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "wasm closure return invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "closure return invoke return shape",
            )),
        }
    }
}

enum RustClosureReturnTokens {
    Void,
    DirectPrimitive { ffi_type: TokenStream },
    DirectPassable { rust_type: Box<Type> },
    NativeEncoded,
    WasmEncoded,
    Fallible(Box<FallibleRustClosureReturn>),
}

impl RustClosureReturnTokens {
    fn fallible(error: EncodedError, success: FallibleSuccess) -> Self {
        Self::Fallible(Box::new(FallibleRustClosureReturn { error, success }))
    }

    fn return_type(&self) -> TokenStream {
        match self {
            Self::Void => TokenStream::new(),
            Self::DirectPrimitive { ffi_type } => quote! { -> #ffi_type },
            Self::DirectPassable { rust_type } => {
                quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out }
            }
            Self::NativeEncoded => quote! { -> ::boltffi::__private::FfiBuf },
            Self::WasmEncoded => quote! { -> u64 },
            Self::Fallible(fallible) => fallible.error.return_type.clone(),
        }
    }

    fn ffi_parameters(&self) -> Vec<TokenStream> {
        match self {
            Self::Fallible(fallible) => fallible.success.ffi_parameters(),
            _ => Vec::new(),
        }
    }

    fn ffi_parameter_types(&self) -> Vec<TokenStream> {
        match self {
            Self::Fallible(fallible) => fallible.success.ffi_parameter_types(),
            _ => Vec::new(),
        }
    }

    fn body(&self, call: TokenStream) -> TokenStream {
        match self {
            Self::Void => quote! {
                #call;
            },
            Self::DirectPrimitive { .. } => quote! { #call },
            Self::DirectPassable { .. } => quote! {
                ::boltffi::__private::Passable::pack(#call)
            },
            Self::NativeEncoded => quote! {
                {
                    let __boltffi_result = #call;
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
                }
            },
            Self::WasmEncoded => quote! {
                {
                    let __boltffi_result = #call;
                    ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result).into_packed()
                }
            },
            Self::Fallible(fallible) => fallible.success.body(&fallible.error, call),
        }
    }

    fn failure(&self) -> TokenStream {
        match self {
            Self::Void => quote! { return; },
            Self::DirectPrimitive { .. } => quote! {
                return ::core::default::Default::default();
            },
            Self::DirectPassable { .. } => quote! {
                return unsafe { ::core::mem::MaybeUninit::zeroed().assume_init() };
            },
            Self::NativeEncoded => quote! {
                return ::boltffi::__private::FfiBuf::default();
            },
            Self::WasmEncoded => quote! {
                return ::boltffi::__private::FfiBuf::default().into_packed();
            },
            Self::Fallible(fallible) => fallible.error.failure(),
        }
    }
}

struct FallibleRustClosureReturn {
    error: EncodedError,
    success: FallibleSuccess,
}

struct EncodedError {
    return_type: TokenStream,
    value: TokenStream,
    empty_value: TokenStream,
}

impl EncodedError {
    fn failure(&self) -> TokenStream {
        let value = &self.value;
        quote! {
            {
                let __boltffi_error = ::boltffi::__private::take_last_error()
                    .unwrap_or_else(|| "closure invoke argument conversion failed".to_string());
                return #value;
            }
        }
    }
}

enum FallibleSuccess {
    Void,
    DirectPrimitive {
        ffi_type: TokenStream,
    },
    DirectPassable {
        rust_type: Box<Type>,
    },
    Encoded {
        out_type: TokenStream,
        value: TokenStream,
    },
}

impl FallibleSuccess {
    fn ffi_parameters(&self) -> Vec<TokenStream> {
        let out = local::Wrapper::new(Span::call_site()).success_out();
        self.ffi_parameter_types()
            .into_iter()
            .map(|ty| quote! { #out: #ty })
            .collect()
    }

    fn ffi_parameter_types(&self) -> Vec<TokenStream> {
        match self {
            Self::Void => Vec::new(),
            Self::DirectPrimitive { ffi_type } => vec![quote! { *mut #ffi_type }],
            Self::DirectPassable { rust_type } => vec![quote! {
                *mut <#rust_type as ::boltffi::__private::Passable>::Out
            }],
            Self::Encoded { out_type, .. } => vec![quote! { *mut #out_type }],
        }
    }

    fn body(&self, error: &EncodedError, call: TokenStream) -> TokenStream {
        let locals = local::Wrapper::new(Span::call_site());
        let success_out = locals.success_out();
        let success_ident = locals.success();
        let empty_error = &error.empty_value;
        let error_value = &error.value;
        let pattern = self.pattern(&success_ident);
        let write_success = self.write_success(&success_ident, &success_out);
        quote! {
            match #call {
                Ok(#pattern) => {
                    #write_success
                    #empty_error
                }
                Err(__boltffi_error) => {
                    #error_value
                }
            }
        }
    }

    fn pattern(&self, success: &Ident) -> TokenStream {
        match self {
            Self::Void => quote! { () },
            _ => quote! { #success },
        }
    }

    fn write_success(&self, success: &Ident, out: &Ident) -> TokenStream {
        match self {
            Self::Void => TokenStream::new(),
            Self::DirectPrimitive { .. } => quote! {
                if !#out.is_null() {
                    unsafe {
                        *#out = #success;
                    }
                }
            },
            Self::DirectPassable { .. } => quote! {
                if !#out.is_null() {
                    unsafe {
                        *#out = ::boltffi::__private::Passable::pack(#success);
                    }
                }
            },
            Self::Encoded { value, .. } => quote! {
                if !#out.is_null() {
                    unsafe {
                        *#out = #value;
                    }
                }
            },
        }
    }
}

struct RustFallibleReturn {
    ok: Type,
}

struct ReturnedClosure {
    kind: ReturnedClosureKind,
    form: ClosureForm,
    signature: ClosureSignature,
}

impl ReturnedClosure {
    fn new<S: Target>(
        source: &FnSig,
        closure: &ClosureReturn<S, OutOfRust>,
        rust_type: Option<&Type>,
    ) -> Result<Self, Error> {
        let kind = match (closure.presence(), closure.form(), rust_type) {
            (HandlePresence::Required, ClosureForm::FunctionPointer, _) => {
                ReturnedClosureKind::FunctionPointer
            }
            (
                HandlePresence::Required,
                ClosureForm::Fn | ClosureForm::FnMut | ClosureForm::FnOnce,
                Some(ty),
            ) if is_boxed_return_type(ty) => ReturnedClosureKind::Boxed,
            (
                HandlePresence::Required,
                ClosureForm::Fn | ClosureForm::FnMut | ClosureForm::FnOnce,
                _,
            ) => ReturnedClosureKind::ImplTrait,
            (
                HandlePresence::Nullable,
                ClosureForm::Fn | ClosureForm::FnMut | ClosureForm::FnOnce,
                _,
            ) => ReturnedClosureKind::NullableBoxed,
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "source closure return form does not match binding closure",
                ));
            }
        };
        let signature = ClosureSignature::from_source(source, closure.form())?;

        Ok(Self {
            kind,
            form: closure.form(),
            signature,
        })
    }

    fn invocation(&self) -> TokenStream {
        match self.form {
            ClosureForm::Fn | ClosureForm::FnMut => quote! { __boltffi_closure },
            ClosureForm::FnOnce => quote! { __boltffi_closure },
            _ => quote! { __boltffi_closure },
        }
    }

    fn context_type(&self) -> TokenStream {
        let trait_object = self.trait_object();
        match self.form {
            ClosureForm::Fn | ClosureForm::FnMut => trait_object,
            ClosureForm::FnOnce => quote! { Option<#trait_object> },
            _ => trait_object,
        }
    }

    fn context_value(&self, value: &Ident) -> Result<TokenStream, Error> {
        let trait_object = self.trait_object();
        Ok(match (self.kind, self.form) {
            (ReturnedClosureKind::ImplTrait, ClosureForm::Fn | ClosureForm::FnMut) => {
                quote! { Box::new(#value) as #trait_object }
            }
            (ReturnedClosureKind::ImplTrait, ClosureForm::FnOnce) => {
                quote! { Some(Box::new(#value) as #trait_object) }
            }
            (ReturnedClosureKind::FunctionPointer, ClosureForm::FunctionPointer) => {
                quote! { Box::new(#value) as #trait_object }
            }
            (ReturnedClosureKind::Boxed, ClosureForm::Fn | ClosureForm::FnMut) => {
                quote! { #value }
            }
            (ReturnedClosureKind::Boxed, ClosureForm::FnOnce) => quote! { Some(#value) },
            (ReturnedClosureKind::NullableBoxed, _) => quote! { #value },
            (_, _) => return Err(Error::UnsupportedExpansion("closure return form")),
        })
    }

    fn write_body(
        &self,
        value: &Ident,
        present: TokenStream,
        absent: TokenStream,
    ) -> Result<TokenStream, Error> {
        match self.kind {
            ReturnedClosureKind::ImplTrait
            | ReturnedClosureKind::FunctionPointer
            | ReturnedClosureKind::Boxed => Ok(present),
            ReturnedClosureKind::NullableBoxed => {
                let context_type = self.context_type();
                let present_value = match self.form {
                    ClosureForm::Fn | ClosureForm::FnMut => quote! { #value },
                    ClosureForm::FnOnce => quote! { Some(#value) },
                    _ => return Err(Error::UnsupportedExpansion("closure return form")),
                };
                Ok(quote! {
                    match #value {
                        Some(#value) => {
                            let #value: #context_type = #present_value;
                            #present
                        }
                        None => {
                            #absent
                        }
                    }
                })
            }
        }
    }

    fn context_binding(&self, context: TokenStream) -> TokenStream {
        match self.form {
            ClosureForm::Fn => quote! { &*(#context) },
            ClosureForm::FnMut => quote! { &mut *(#context) },
            ClosureForm::FnOnce => quote! {
                (&mut *(#context)).take().expect("closure already invoked")
            },
            _ => quote! { &*(#context) },
        }
    }

    fn trait_object(&self) -> TokenStream {
        let trait_ident = self.form.trait_ident();
        let parameters = &self.signature.parameters;
        let return_type = self.signature.return_tokens();
        quote! { Box<dyn #trait_ident(#(#parameters),*) #return_type + 'static> }
    }
}

#[derive(Clone, Copy)]
enum ReturnedClosureKind {
    ImplTrait,
    FunctionPointer,
    Boxed,
    NullableBoxed,
}

fn is_boxed_return_type(rust_type: &Type) -> bool {
    let Type::Path(type_path) = rust_type else {
        return false;
    };
    type_path
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Box")
}

trait ClosureFormTokens {
    fn trait_ident(self) -> Ident;
}

impl ClosureFormTokens for ClosureForm {
    fn trait_ident(self) -> Ident {
        match self {
            ClosureForm::Fn => format_ident!("Fn"),
            ClosureForm::FnMut => format_ident!("FnMut"),
            ClosureForm::FnOnce => format_ident!("FnOnce"),
            _ => format_ident!("Fn"),
        }
    }
}

struct ClosureSignature {
    form: ClosureForm,
    parameters: Vec<Type>,
    return_type: Option<Type>,
}

impl ClosureSignature {
    fn from_source(source: &FnSig, form: ClosureForm) -> Result<Self, Error> {
        let parameters = source
            .parameters
            .iter()
            .map(signature::rust_type)
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = match &source.returns {
            ReturnDef::Void => None,
            ReturnDef::Value(type_expr) => Some(signature::rust_type(type_expr)?),
        };
        Ok(Self {
            form,
            parameters,
            return_type,
        })
    }

    fn return_tokens(&self) -> TokenStream {
        match &self.return_type {
            Some(ty) => quote! { -> #ty },
            None => TokenStream::new(),
        }
    }
}

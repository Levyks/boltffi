use boltffi_ast::{FnSig, ReturnDef, TypeExpr};
use boltffi_binding::{
    ClosureForm, ClosureParameter, ClosureReturn, ErrorDecl, HandlePresence, ImportedCallable,
    IntoRust, Native, OutgoingParam, ParamPlan, ReturnPlan, TypeRef, Wasm32, WritePlan, native,
    wasm32,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{self, Render, encoded, names},
};

use super::Tokens;

pub struct Renderer;

pub struct Input<'expansion, 'lowered, S: Target> {
    closure: ForeignClosure<'lowered, S>,
    source: rust_api::Closure<'lowered>,
    ident: Ident,
    failure: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: Target> Input<'expansion, 'lowered, S> {
    pub fn new(
        closure: &'lowered ClosureParameter<S, IntoRust>,
        source: rust_api::Closure<'lowered>,
        ident: Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            closure: ForeignClosure::Parameter(closure),
            source,
            ident,
            failure,
            expansion,
        }
    }

    pub fn returned(
        closure: &'lowered ClosureReturn<S, IntoRust>,
        source: rust_api::Closure<'lowered>,
        ident: Ident,
        failure: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            closure: ForeignClosure::Return(closure),
            source,
            ident,
            failure,
            expansion,
        }
    }
}

#[derive(Clone, Copy)]
enum ForeignClosure<'lowered, S: Target> {
    Parameter(&'lowered ClosureParameter<S, IntoRust>),
    Return(&'lowered ClosureReturn<S, IntoRust>),
}

impl<'lowered, S: Target> ForeignClosure<'lowered, S> {
    fn form(self) -> ClosureForm {
        match self {
            Self::Parameter(closure) => closure.form(),
            Self::Return(closure) => closure.form(),
        }
    }

    fn presence(self) -> HandlePresence {
        match self {
            Self::Parameter(closure) => closure.presence(),
            Self::Return(closure) => closure.presence(),
        }
    }

    fn registration(self) -> &'lowered boltffi_binding::ClosureRegistration<S, IntoRust> {
        match self {
            Self::Parameter(closure) => closure.registration(),
            Self::Return(closure) => closure.registration(),
        }
    }

    fn invoke(self) -> &'lowered ImportedCallable<S> {
        match self {
            Self::Parameter(closure) => closure.invoke(),
            Self::Return(closure) => closure.invoke(),
        }
    }
}

impl<'expansion, 'lowered> Render<Native, Input<'expansion, 'lowered, Native>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        NativeClosure::new(input).tokens()
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Input<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        WasmClosure::new(input).tokens()
    }
}

struct NativeClosure<'expansion, 'lowered> {
    input: Input<'expansion, 'lowered, Native>,
}

impl<'expansion, 'lowered> NativeClosure<'expansion, 'lowered> {
    fn new(input: Input<'expansion, 'lowered, Native>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        match self.input.closure.registration().shape() {
            native::ClosureRegistration::InvokeContextRelease => self.invoke_context(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure registration",
            )),
        }
    }

    fn invoke_context(self) -> Result<Tokens, Error> {
        let ident = &self.input.ident;
        let closure_binding = ClosureBinding::new(
            self.input.source,
            self.input.closure.form(),
            self.input.closure.presence(),
        )?;
        let invoke = ClosureInvoke::<Native>::new(
            self.input.closure.invoke(),
            self.input.source.signature(),
            &closure_binding,
            self.input.expansion,
        )?;
        let invoke_parameters = invoke.parameters()?;
        let names = names::ClosureRegistration::new(ident);
        let callback = names.call();
        let context = names.context();
        let release = names.release();
        let owner = names.owner();
        let return_tokens = invoke.return_tokens()?;
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let return_call_arguments = return_tokens.call_arguments();
        let return_type = return_tokens.ffi_return_type();
        let setup = &invoke_parameters.setup;
        let call_arguments = &invoke_parameters.call_arguments;
        let call = quote! {
            #(#setup)*
            #callback(#owner.context() #(, #call_arguments)* #(, #return_call_arguments)*)
        };
        let body = return_tokens.body(call);
        let closure = closure_binding.native_binding(NativeBinding {
            ident,
            callback: &callback,
            context: &context,
            release: &release,
            owner: &owner,
            rust_parameters: &invoke_parameters.rust_parameters,
            body,
            failure: &self.input.failure,
        })?;
        let function_pointer_type = closure_binding.native_function_pointer_type(
            &invoke_parameters
                .ffi_parameter_types
                .iter()
                .cloned()
                .chain(return_ffi_parameter_types)
                .collect::<Vec<_>>(),
            return_type.clone(),
        )?;
        let release_type = closure_binding.native_release_function_type();

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![
                quote! { #callback: #function_pointer_type },
                quote! { #context: *mut ::core::ffi::c_void },
                quote! { #release: #release_type },
            ],
            ffi_parameter_types: vec![
                function_pointer_type,
                quote! { *mut ::core::ffi::c_void },
                release_type,
            ],
            conversions: vec![closure],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

impl ClosureBinding {
    fn native_release_function_type(&self) -> TokenStream {
        match self {
            Self::NullableBoxed(_, _) => quote! {
                Option<unsafe extern "C" fn(*mut ::core::ffi::c_void)>
            },
            _ => quote! {
                unsafe extern "C" fn(*mut ::core::ffi::c_void)
            },
        }
    }
}

impl ClosureBinding {
    fn native_function_pointer_type(
        &self,
        ffi_parameter_types: &[TokenStream],
        return_type: TokenStream,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::NullableBoxed(_, _) => Ok(quote! {
                Option<extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type>
            }),
            _ => Ok(quote! {
                extern "C" fn(*mut ::core::ffi::c_void #(, #ffi_parameter_types)*) #return_type
            }),
        }
    }
}

struct WasmClosure<'expansion, 'lowered> {
    input: Input<'expansion, 'lowered, Wasm32>,
}

impl<'expansion, 'lowered> WasmClosure<'expansion, 'lowered> {
    fn new(input: Input<'expansion, 'lowered, Wasm32>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.input.ident;
        let closure_binding = ClosureBinding::new(
            self.input.source,
            self.input.closure.form(),
            self.input.closure.presence(),
        )?;
        let invoke = ClosureInvoke::<Wasm32>::new(
            self.input.closure.invoke(),
            self.input.source.signature(),
            &closure_binding,
            self.input.expansion,
        )?;
        let invoke_parameters = invoke.parameters()?;
        let return_tokens = invoke.return_tokens()?;
        let registration = self.input.closure.registration().shape();
        let call = Ident::new(registration.call().name().as_str(), ident.span());
        let free = Ident::new(registration.free().name().as_str(), ident.span());
        let names = names::ClosureRegistration::new(ident);
        let owner = names.owner();
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let return_call_arguments = return_tokens.call_arguments();
        let return_type = return_tokens.ffi_return_type();
        let setup = &invoke_parameters.setup;
        let call_arguments = &invoke_parameters.call_arguments;
        let call_body = quote! {
            #(#setup)*
            #call(#owner.handle() #(, #call_arguments)* #(, #return_call_arguments)*)
        };
        let body = return_tokens.body(call_body);
        let closure = closure_binding.wasm_binding(
            ident,
            &owner,
            &free,
            &invoke_parameters.rust_parameters,
            body,
            &self.input.failure,
        )?;

        let ffi_parameter_types = invoke_parameters
            .ffi_parameter_types
            .iter()
            .cloned()
            .chain(return_ffi_parameter_types)
            .collect::<Vec<_>>();
        let ffi_parameters = ffi_parameter_types
            .iter()
            .enumerate()
            .map(|(index, parameter_type)| {
                let name = names::ClosureArgument::new(index).ffi();
                quote! { #name: #parameter_type }
            })
            .collect::<Vec<_>>();

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: u32 }],
            ffi_parameter_types: vec![quote! { u32 }],
            conversions: vec![quote! {
                unsafe extern "C" {
                    fn #call(handle: u32 #(, #ffi_parameters)*) #return_type;
                    fn #free(handle: u32);
                }
                #closure
            }],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }
}

struct ClosureInvoke<'expansion, 'lowered, 'rust, S: Target> {
    callable: &'lowered ImportedCallable<S>,
    source: &'lowered FnSig,
    closure_binding: &'rust ClosureBinding,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, 'rust, S: Target> ClosureInvoke<'expansion, 'lowered, 'rust, S> {
    fn new(
        callable: &'lowered ImportedCallable<S>,
        source: &'lowered FnSig,
        closure_binding: &'rust ClosureBinding,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Self, Error> {
        if callable.params().len() != source.parameters.len() {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter count does not match binding invoke parameter count",
            ));
        }
        Ok(Self {
            callable,
            source,
            closure_binding,
            expansion,
        })
    }

    fn parameters(&self) -> Result<InvokeParameters, Error> {
        let tokens = self
            .callable
            .params()
            .iter()
            .zip(self.source.parameters.iter())
            .zip(self.closure_binding.parameters().iter())
            .enumerate()
            .map(|(index, ((param, source), rust_type))| {
                InvokeParameterInput::new(index, param.payload(), source, rust_type, self.expansion)
                    .tokens()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(InvokeParameters::from(tokens))
    }

    fn return_tokens(&self) -> Result<ForeignClosureReturnTokens<'rust>, Error>
    where
        ForeignClosureReturnRenderer: Render<
                S,
                ForeignClosureReturn<'expansion, 'lowered, 'rust, S>,
                Output = ForeignClosureReturnTokens<'rust>,
            >,
    {
        <ForeignClosureReturnRenderer as Render<
            S,
            ForeignClosureReturn<'expansion, 'lowered, 'rust, S>,
        >>::render(
            ForeignClosureReturnRenderer,
            ForeignClosureReturn::new(
                self.callable.returns().plan(),
                self.callable.error(),
                &self.source.returns,
                self.closure_binding.return_type(),
                self.expansion,
            ),
        )
    }
}

struct InvokeParameterInput<'expansion, 'lowered, 'rust, S: Target> {
    index: usize,
    payload: &'lowered OutgoingParam<S>,
    source: &'lowered TypeExpr,
    rust_type: &'rust Type,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, 'rust, S: Target> InvokeParameterInput<'expansion, 'lowered, 'rust, S> {
    fn new(
        index: usize,
        payload: &'lowered OutgoingParam<S>,
        source: &'lowered TypeExpr,
        rust_type: &'rust Type,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            index,
            payload,
            source,
            rust_type,
            expansion,
        }
    }

    fn tokens(self) -> Result<InvokeParameterTokens, Error> {
        let argument = names::ClosureArgument::new(self.index).value();
        let rust_type = self.rust_type;
        match self.payload {
            OutgoingParam::Value(ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                ..
            }) => {
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <wrapper::type_ref::Renderer as Render<S, &TypeRef>>::render(
                    wrapper::type_ref::Renderer,
                    &ty,
                )?;
                Ok(InvokeParameterTokens {
                    rust_parameter: quote! { #argument: #rust_type },
                    ffi_parameter_types: vec![ffi_type],
                    setup: Vec::new(),
                    call_arguments: vec![quote! { #argument }],
                })
            }
            OutgoingParam::Value(ParamPlan::Direct { .. }) => Ok(InvokeParameterTokens {
                rust_parameter: quote! { #argument: #rust_type },
                ffi_parameter_types: vec![quote! {
                    <#rust_type as ::boltffi::__private::Passable>::Out
                }],
                setup: Vec::new(),
                call_arguments: vec![quote! {
                    ::boltffi::__private::Passable::pack(#argument)
                }],
            }),
            OutgoingParam::Value(ParamPlan::Encoded { codec, .. }) => {
                let locals = names::ClosureArgument::new(self.index);
                let wire = locals.wire();
                let pointer = locals.pointer();
                let length = locals.length();
                let buffer = encoded::outgoing::Value::new(codec.root(), self.expansion)
                    .buffer(quote! { #argument })?;
                Ok(InvokeParameterTokens {
                    rust_parameter: quote! { #argument: #rust_type },
                    ffi_parameter_types: vec![quote! { *const u8 }, quote! { usize }],
                    setup: vec![quote! {
                        let #wire = #buffer;
                        let #pointer = #wire.as_ptr();
                        let #length = #wire.len();
                    }],
                    call_arguments: vec![quote! { #pointer }, quote! { #length }],
                })
            }
            OutgoingParam::Value(_) => Err(Error::UnsupportedExpansion(
                "closure invoke parameter shape",
            )),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedExpansion(
                "nested closure invoke parameter",
            )),
        }
    }
}

struct InvokeParameterTokens {
    rust_parameter: TokenStream,
    ffi_parameter_types: Vec<TokenStream>,
    setup: Vec<TokenStream>,
    call_arguments: Vec<TokenStream>,
}

struct InvokeParameters {
    rust_parameters: Vec<TokenStream>,
    ffi_parameter_types: Vec<TokenStream>,
    setup: Vec<TokenStream>,
    call_arguments: Vec<TokenStream>,
}

impl From<Vec<InvokeParameterTokens>> for InvokeParameters {
    fn from(tokens: Vec<InvokeParameterTokens>) -> Self {
        InvokeParameters {
            rust_parameters: tokens
                .iter()
                .map(|token| token.rust_parameter.clone())
                .collect(),
            ffi_parameter_types: tokens
                .iter()
                .flat_map(|token| token.ffi_parameter_types.iter().cloned())
                .collect(),
            setup: tokens
                .iter()
                .flat_map(|token| token.setup.iter().cloned())
                .collect(),
            call_arguments: tokens
                .iter()
                .flat_map(|token| token.call_arguments.iter().cloned())
                .collect(),
        }
    }
}

struct ForeignClosureReturn<'expansion, 'lowered, 'rust, S: Target> {
    plan: &'lowered ReturnPlan<S, IntoRust>,
    error: &'lowered ErrorDecl<S, IntoRust>,
    source: &'lowered ReturnDef,
    rust_type: Option<&'rust Type>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, 'rust, S: Target> ForeignClosureReturn<'expansion, 'lowered, 'rust, S> {
    fn new(
        plan: &'lowered ReturnPlan<S, IntoRust>,
        error: &'lowered ErrorDecl<S, IntoRust>,
        source: &'lowered ReturnDef,
        rust_type: Option<&'rust Type>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            plan,
            error,
            source,
            rust_type,
            expansion,
        }
    }

    fn direct_tokens(&self) -> Result<Option<ForeignClosureReturnTokens<'rust>>, Error> {
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
                Ok(Some(ForeignClosureReturnTokens::Void))
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
                let ffi_type = <wrapper::type_ref::Renderer as Render<S, &TypeRef>>::render(
                    wrapper::type_ref::Renderer,
                    &ty,
                )?;
                Ok(Some(ForeignClosureReturnTokens::DirectPrimitive {
                    ffi_type,
                }))
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                if !matches!(self.source, ReturnDef::Value(_)) {
                    return Err(Error::SourceSyntaxMismatch(
                        "source closure invoke return does not match binding return plan",
                    ));
                }
                let rust_type = self.rust_type.ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke direct return requires source return type",
                ))?;
                Ok(Some(ForeignClosureReturnTokens::DirectPassable {
                    rust_type,
                }))
            }
            ReturnPlan::EncodedViaReturnSlot { .. } => Ok(None),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }

    fn rust_fallible_return(&self) -> Result<RustFallibleReturn, Error> {
        let fallible = rust_api::Return::new(self.source).fallible()?;
        Ok(RustFallibleReturn {
            ok: fallible.ok_written_type()?,
            err: fallible.error_written_type()?,
        })
    }

    fn encoded_expression(
        &self,
        codec: &'lowered WritePlan,
        rust_type: &Type,
        bytes: TokenStream,
    ) -> Result<TokenStream, Error> {
        encoded::incoming::Value::new(codec.root(), self.expansion).expression(
            encoded::incoming::Bytes::new(
                rust_type,
                bytes,
                quote! { panic!("closure encoded return conversion failed: {}", error) },
            ),
        )
    }
}

struct ForeignClosureReturnRenderer;

impl<'expansion, 'lowered, 'rust>
    Render<Native, ForeignClosureReturn<'expansion, 'lowered, 'rust, Native>>
    for ForeignClosureReturnRenderer
{
    type Output = ForeignClosureReturnTokens<'rust>;

    fn render(
        self,
        input: ForeignClosureReturn<'expansion, 'lowered, 'rust, Native>,
    ) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
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
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <wrapper::type_ref::Renderer as Render<Native, &TypeRef>>::render(
                    wrapper::type_ref::Renderer,
                    &ty,
                )?;
                let error_type = input.rust_fallible_return()?.err;
                let error = input.encoded_expression(
                    codec,
                    &error_type,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleDirectPrimitive { ffi_type, error })
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                let error = input.encoded_expression(
                    codec,
                    &result.err,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleDirectPassable {
                    ok_type: result.ok,
                    error,
                })
            }
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
                let result = input.rust_fallible_return()?;
                let ok = input.encoded_expression(
                    ok_codec,
                    &result.ok,
                    quote! { __boltffi_success_bytes },
                )?;
                let error = input.encoded_expression(
                    error_codec,
                    &result.err,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleEncoded {
                    ok_type: result.ok,
                    ok,
                    error,
                })
            }
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let error_type = input.rust_fallible_return()?.err;
                let error = input.encoded_expression(
                    codec,
                    &error_type,
                    quote! { __boltffi_error_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleVoid { error })
            }
            (
                ReturnPlan::EncodedViaReturnSlot {
                    codec,
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let rust_type = input.rust_type.ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke encoded return requires source return type",
                ))?;
                let value = input.encoded_expression(
                    codec,
                    rust_type,
                    quote! { __boltffi_result_bytes },
                )?;
                Ok(ForeignClosureReturnTokens::NativeEncoded { value })
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "native closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }
}

impl<'expansion, 'lowered, 'rust>
    Render<Wasm32, ForeignClosureReturn<'expansion, 'lowered, 'rust, Wasm32>>
    for ForeignClosureReturnRenderer
{
    type Output = ForeignClosureReturnTokens<'rust>;

    fn render(
        self,
        input: ForeignClosureReturn<'expansion, 'lowered, 'rust, Wasm32>,
    ) -> Result<Self::Output, Error> {
        if let Some(tokens) = input.direct_tokens()? {
            return Ok(tokens);
        }

        match (input.plan, input.error) {
            (
                ReturnPlan::DirectViaOutPointer {
                    ty: TypeRef::Primitive(primitive),
                },
                ErrorDecl::EncodedViaReturnSlot {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => {
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <wrapper::type_ref::Renderer as Render<Wasm32, &TypeRef>>::render(
                    wrapper::type_ref::Renderer,
                    &ty,
                )?;
                Ok(ForeignClosureReturnTokens::WasmFallibleDirectPrimitive { ffi_type })
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(ForeignClosureReturnTokens::WasmFallibleDirectPassable {
                ok_type: input.rust_fallible_return()?.ok,
            }),
            (
                ReturnPlan::EncodedViaOutPointer {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(ForeignClosureReturnTokens::WasmFalliblePackedString {
                ok_type: input.rust_fallible_return()?.ok,
            }),
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
            ) => Ok(ForeignClosureReturnTokens::WasmFallibleVoidString),
            (
                ReturnPlan::EncodedViaReturnSlot {
                    ty: TypeRef::String,
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::None(_),
            ) => Ok(ForeignClosureReturnTokens::WasmPackedString),
            (
                ReturnPlan::EncodedViaReturnSlot {
                    shape: wasm32::BufferShape::Packed,
                    ..
                },
                ErrorDecl::None(_),
            ) => Err(Error::UnsupportedExpansion(
                "wasm closure invoke encoded return",
            )),
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "wasm closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }
}

enum ForeignClosureReturnTokens<'rust> {
    Void,
    DirectPrimitive {
        ffi_type: TokenStream,
    },
    DirectPassable {
        rust_type: &'rust Type,
    },
    NativeEncoded {
        value: TokenStream,
    },
    WasmPackedString,
    NativeFallibleVoid {
        error: TokenStream,
    },
    NativeFallibleDirectPrimitive {
        ffi_type: TokenStream,
        error: TokenStream,
    },
    NativeFallibleDirectPassable {
        ok_type: Type,
        error: TokenStream,
    },
    NativeFallibleEncoded {
        ok_type: Type,
        ok: TokenStream,
        error: TokenStream,
    },
    WasmFallibleVoidString,
    WasmFallibleDirectPrimitive {
        ffi_type: TokenStream,
    },
    WasmFallibleDirectPassable {
        ok_type: Type,
    },
    WasmFalliblePackedString {
        ok_type: Type,
    },
}

impl ForeignClosureReturnTokens<'_> {
    fn ffi_return_type(&self) -> TokenStream {
        match self {
            Self::Void => TokenStream::new(),
            Self::DirectPrimitive { ffi_type } => quote! { -> #ffi_type },
            Self::DirectPassable { rust_type } => {
                quote! { -> <#rust_type as ::boltffi::__private::Passable>::In }
            }
            Self::NativeEncoded { .. } => quote! { -> ::boltffi::__private::FfiBuf },
            Self::WasmPackedString => quote! { -> u64 },
            Self::NativeFallibleVoid { .. }
            | Self::NativeFallibleDirectPrimitive { .. }
            | Self::NativeFallibleDirectPassable { .. }
            | Self::NativeFallibleEncoded { .. } => quote! { -> ::boltffi::__private::FfiBuf },
            Self::WasmFallibleVoidString
            | Self::WasmFallibleDirectPrimitive { .. }
            | Self::WasmFallibleDirectPassable { .. }
            | Self::WasmFalliblePackedString { .. } => quote! { -> u64 },
        }
    }

    fn ffi_parameter_types(&self) -> Vec<TokenStream> {
        match self {
            Self::NativeFallibleDirectPrimitive { ffi_type, .. }
            | Self::WasmFallibleDirectPrimitive { ffi_type } => {
                vec![quote! { *mut #ffi_type }]
            }
            Self::NativeFallibleDirectPassable { ok_type, .. }
            | Self::WasmFallibleDirectPassable { ok_type } => {
                vec![quote! { *mut <#ok_type as ::boltffi::__private::Passable>::In }]
            }
            Self::NativeFallibleEncoded { .. } => {
                vec![quote! { *mut ::boltffi::__private::FfiBuf }]
            }
            Self::WasmFalliblePackedString { .. } => vec![quote! { *mut u64 }],
            _ => Vec::new(),
        }
    }

    fn setup(&self) -> Vec<TokenStream> {
        match self {
            Self::NativeFallibleDirectPrimitive { ffi_type, .. }
            | Self::WasmFallibleDirectPrimitive { ffi_type } => vec![quote! {
                let mut __boltffi_success = ::core::mem::MaybeUninit::<#ffi_type>::uninit();
            }],
            Self::NativeFallibleDirectPassable { ok_type, .. }
            | Self::WasmFallibleDirectPassable { ok_type } => vec![quote! {
                let mut __boltffi_success =
                    ::core::mem::MaybeUninit::<<#ok_type as ::boltffi::__private::Passable>::In>::uninit();
            }],
            Self::NativeFallibleEncoded { .. } => vec![quote! {
                let mut __boltffi_success =
                    ::core::mem::MaybeUninit::<::boltffi::__private::FfiBuf>::uninit();
            }],
            Self::WasmFalliblePackedString { .. } => vec![quote! {
                let mut __boltffi_success = ::core::mem::MaybeUninit::<u64>::uninit();
            }],
            _ => Vec::new(),
        }
    }

    fn call_arguments(&self) -> Vec<TokenStream> {
        match self {
            Self::NativeFallibleDirectPrimitive { .. }
            | Self::NativeFallibleDirectPassable { .. }
            | Self::NativeFallibleEncoded { .. }
            | Self::WasmFallibleDirectPrimitive { .. }
            | Self::WasmFallibleDirectPassable { .. }
            | Self::WasmFalliblePackedString { .. } => {
                vec![quote! { __boltffi_success.as_mut_ptr() }]
            }
            _ => Vec::new(),
        }
    }

    fn body(&self, call: TokenStream) -> TokenStream {
        match self {
            Self::Void => quote! {
                unsafe {
                    #call;
                }
            },
            Self::DirectPrimitive { .. } => quote! { unsafe { #call } },
            Self::DirectPassable { rust_type } => quote! {
                unsafe {
                    <#rust_type as ::boltffi::__private::Passable>::unpack(#call)
                }
            },
            Self::NativeEncoded { value } => quote! {
                {
                    let __boltffi_result_buf = unsafe { #call };
                    let __boltffi_result_bytes = unsafe {
                        __boltffi_result_buf.as_byte_slice()
                    };
                    #value
                }
            },
            Self::WasmPackedString => quote! {
                {
                    let __boltffi_result_packed = unsafe { #call };
                    unsafe {
                        ::boltffi::__private::take_packed_utf8_string(__boltffi_result_packed)
                    }
                }
            },
            Self::NativeFallibleVoid { error } => quote! {
                {
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(())
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            },
            Self::NativeFallibleDirectPrimitive { error, .. } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(unsafe { __boltffi_success.assume_init() })
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            }
            Self::NativeFallibleDirectPassable { ok_type, error } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(unsafe {
                            <#ok_type as ::boltffi::__private::Passable>::unpack(
                                __boltffi_success.assume_init()
                            )
                        })
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            }
            Self::NativeFallibleEncoded {
                ok_type: _,
                ok,
                error,
            } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        let __boltffi_success_buf = unsafe {
                            __boltffi_success.assume_init()
                        };
                        let __boltffi_success_bytes = unsafe {
                            __boltffi_success_buf.as_byte_slice()
                        };
                        Ok(#ok)
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(#error)
                    }
                }
            }
            Self::WasmFallibleVoidString => quote! {
                {
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(())
                    } else {
                        Err(unsafe {
                            ::boltffi::__private::take_packed_utf8_string(__boltffi_error_packed)
                        })
                    }
                }
            },
            Self::WasmFallibleDirectPrimitive { .. } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(unsafe { __boltffi_success.assume_init() })
                    } else {
                        Err(unsafe {
                            ::boltffi::__private::take_packed_utf8_string(__boltffi_error_packed)
                        })
                    }
                }
            }
            Self::WasmFallibleDirectPassable { ok_type } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(unsafe {
                            <#ok_type as ::boltffi::__private::Passable>::unpack(
                                __boltffi_success.assume_init()
                            )
                        })
                    } else {
                        Err(unsafe {
                            ::boltffi::__private::take_packed_utf8_string(__boltffi_error_packed)
                        })
                    }
                }
            }
            Self::WasmFalliblePackedString { .. } => {
                let setup = self.setup();
                quote! {
                    #(#setup)*
                    let __boltffi_error_packed = unsafe { #call };
                    if __boltffi_error_packed == 0 {
                        Ok(unsafe {
                            ::boltffi::__private::take_packed_utf8_string(
                                __boltffi_success.assume_init()
                            )
                        })
                    } else {
                        Err(unsafe {
                            ::boltffi::__private::take_packed_utf8_string(__boltffi_error_packed)
                        })
                    }
                }
            }
        }
    }
}

struct RustFallibleReturn {
    ok: Type,
    err: Type,
}

enum ClosureBinding {
    ImplTrait(ClosureSignature),
    Boxed(ClosureSignature, Type),
    NullableBoxed(ClosureSignature, Type),
}

impl ClosureBinding {
    fn new(
        source: rust_api::Closure<'_>,
        closure_form: ClosureForm,
        closure_presence: HandlePresence,
    ) -> Result<Self, Error> {
        if source.function() != closure_form {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter form does not match binding closure",
            ));
        }
        let signature = ClosureSignature::from_source(source.signature(), closure_form)?;
        let closure_binding = match (closure_presence, source.form()) {
            (HandlePresence::Required, rust_api::ClosureSourceForm::BoxedDyn) => {
                Ok(Self::Boxed(signature, source.ty()?))
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::ImplTrait) => {
                Ok(Self::ImplTrait(signature))
            }
            (HandlePresence::Nullable, rust_api::ClosureSourceForm::NullableBoxedDyn) => {
                Ok(Self::NullableBoxed(signature, source.ty()?))
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::FunctionPointer) => Err(
                Error::UnsupportedExpansion("function-pointer closure parameter"),
            ),
            _ => Err(Error::SourceSyntaxMismatch(
                "source closure parameter form does not match binding closure",
            )),
        }?;
        Ok(closure_binding)
    }

    fn parameters(&self) -> &[Type] {
        match self {
            Self::ImplTrait(signature)
            | Self::Boxed(signature, _)
            | Self::NullableBoxed(signature, _) => &signature.parameters,
        }
    }

    fn return_type(&self) -> Option<&Type> {
        match self {
            Self::ImplTrait(signature)
            | Self::Boxed(signature, _)
            | Self::NullableBoxed(signature, _) => signature.return_type.as_ref(),
        }
    }

    fn native_binding(&self, input: NativeBinding<'_>) -> Result<TokenStream, Error> {
        let NativeBinding {
            ident,
            callback,
            context,
            release,
            owner,
            rust_parameters,
            body,
            failure,
        } = input;
        match self {
            Self::ImplTrait(_) => Ok(quote! {
                let #owner = ::boltffi::__private::NativeCallbackOwner::new(#context, #release);
                let #ident = move |#(#rust_parameters),*| {
                    #body
                };
            }),
            Self::Boxed(_, ty) => Ok(quote! {
                let #owner = ::boltffi::__private::NativeCallbackOwner::new(#context, #release);
                let #ident: #ty = Box::new(move |#(#rust_parameters),*| {
                    #body
                });
            }),
            Self::NullableBoxed(_, ty) => Ok(quote! {
                let #ident: #ty = match (#callback, #release) {
                    (Some(#callback), Some(#release)) => {
                        let #owner = ::boltffi::__private::NativeCallbackOwner::new(#context, #release);
                        Some(Box::new(move |#(#rust_parameters),*| {
                            #body
                        }) as _)
                    }
                    (None, None) => None,
                    _ => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: invalid nullable closure registration",
                            stringify!(#ident)
                        ));
                        #failure
                    }
                };
            }),
        }
    }

    fn wasm_binding(
        &self,
        ident: &Ident,
        owner: &Ident,
        free: &Ident,
        rust_parameters: &[TokenStream],
        body: TokenStream,
        failure: &TokenStream,
    ) -> Result<TokenStream, Error> {
        match self {
            Self::ImplTrait(_) => Ok(quote! {
                if #ident == 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null closure handle",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let #owner = ::boltffi::__private::WasmCallbackOwner::new(#ident, #free);
                let #ident = move |#(#rust_parameters),*| {
                    #body
                };
            }),
            Self::Boxed(_, ty) => Ok(quote! {
                if #ident == 0 {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null closure handle",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let #owner = ::boltffi::__private::WasmCallbackOwner::new(#ident, #free);
                let #ident: #ty = Box::new(move |#(#rust_parameters),*| {
                    #body
                });
            }),
            Self::NullableBoxed(_, ty) => Ok(quote! {
                let #ident: #ty = if #ident == 0 {
                    None
                } else {
                    let #owner = ::boltffi::__private::WasmCallbackOwner::new(#ident, #free);
                    Some(Box::new(move |#(#rust_parameters),*| {
                        #body
                    }) as _)
                };
            }),
        }
    }
}

struct NativeBinding<'tokens> {
    ident: &'tokens Ident,
    callback: &'tokens Ident,
    context: &'tokens Ident,
    release: &'tokens Ident,
    owner: &'tokens Ident,
    rust_parameters: &'tokens [TokenStream],
    body: TokenStream,
    failure: &'tokens TokenStream,
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
            .map(|type_expr| {
                rust_api::TypeTokens::new(type_expr).map(rust_api::TypeTokens::into_type)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let return_type = match &source.returns {
            ReturnDef::Void => None,
            ReturnDef::Value(type_expr) => Some(rust_api::TypeTokens::new(type_expr)?.into_type()),
        };
        Ok(Self {
            form,
            parameters,
            return_type,
        })
    }
}

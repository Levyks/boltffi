use boltffi_ast::{ClosureType, ReturnDef};
use boltffi_binding::{
    ClosureParameter, ErrorDecl, HandlePresence, ImportedCallable, IntoRust, Native, OutgoingParam,
    ParamPlan, ReturnPlan, TypeRef, Wasm32, native, wasm32,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{GenericArgument, Ident, PathArguments, ReturnType, Type, TypeImplTrait, TypeParamBound};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, callable::signature, codec, local},
    target::Target,
};

use super::Tokens;

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    closure: &'binding ClosureParameter<S, IntoRust>,
    source: &'binding ClosureType,
    rust_type: &'syntax Type,
    ident: &'syntax Ident,
    failure: TokenStream,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        closure: &'binding ClosureParameter<S, IntoRust>,
        source: &'binding ClosureType,
        rust_type: &'syntax Type,
        ident: &'syntax Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            closure,
            source,
            rust_type,
            ident,
            failure,
        }
    }
}

impl<'binding, 'syntax> RenderRule<Native, Input<'binding, 'syntax, Native>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, Native>) -> Result<Self::Output, Error> {
        NativeClosure::new(input).tokens()
    }
}

impl<'binding, 'syntax> RenderRule<Wasm32, Input<'binding, 'syntax, Wasm32>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax, Wasm32>) -> Result<Self::Output, Error> {
        WasmClosure::new(input).tokens()
    }
}

struct NativeClosure<'binding, 'syntax> {
    input: Input<'binding, 'syntax, Native>,
}

impl<'binding, 'syntax> NativeClosure<'binding, 'syntax> {
    fn new(input: Input<'binding, 'syntax, Native>) -> Self {
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
        let ident = self.input.ident;
        let syntax =
            ClosureSyntax::new(self.input.rust_type, self.input.source, self.input.closure)?;
        let invoke =
            ClosureInvoke::<Native>::new(self.input.closure.invoke(), self.input.source, &syntax)?;
        let invoke_parameters = invoke.parameters()?;
        let names = RegistrationNames::new(ident);
        let callback = &names.call;
        let context = &names.context;
        let release = &names.release;
        let owner = &names.owner;
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
        let closure = syntax.native_binding(NativeBinding {
            ident,
            callback,
            context,
            release,
            owner,
            rust_parameters: &invoke_parameters.rust_parameters,
            body,
            failure: &self.input.failure,
        })?;
        let function_pointer_type = syntax.native_function_pointer_type(
            &invoke_parameters
                .ffi_parameter_types
                .iter()
                .cloned()
                .chain(return_ffi_parameter_types)
                .collect::<Vec<_>>(),
            return_type.clone(),
        )?;
        let release_type = syntax.native_release_function_type();

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

impl ClosureSyntax<'_> {
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

impl ClosureSyntax<'_> {
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

struct WasmClosure<'binding, 'syntax> {
    input: Input<'binding, 'syntax, Wasm32>,
}

impl<'binding, 'syntax> WasmClosure<'binding, 'syntax> {
    fn new(input: Input<'binding, 'syntax, Wasm32>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = self.input.ident;
        let syntax =
            ClosureSyntax::new(self.input.rust_type, self.input.source, self.input.closure)?;
        let invoke =
            ClosureInvoke::<Wasm32>::new(self.input.closure.invoke(), self.input.source, &syntax)?;
        let invoke_parameters = invoke.parameters()?;
        let return_tokens = invoke.return_tokens()?;
        let registration = self.input.closure.registration().shape();
        let call = Ident::new(registration.call().name().as_str(), ident.span());
        let free = Ident::new(registration.free().name().as_str(), ident.span());
        let names = RegistrationNames::new(ident);
        let owner = &names.owner;
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
        let closure = syntax.wasm_binding(
            ident,
            owner,
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
                let name = local::ClosureArgument::new(index).ffi();
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

struct RegistrationNames {
    call: Ident,
    context: Ident,
    release: Ident,
    owner: Ident,
}

impl RegistrationNames {
    fn new(ident: &Ident) -> Self {
        let ident_text = ident.to_string();
        let stem = ident_text.strip_prefix("__boltffi_").unwrap_or(&ident_text);
        Self {
            call: Ident::new(&format!("__boltffi_{stem}_call"), ident.span()),
            context: Ident::new(&format!("__boltffi_{stem}_context"), ident.span()),
            release: Ident::new(&format!("__boltffi_{stem}_release"), ident.span()),
            owner: Ident::new(&format!("__boltffi_{stem}_owner"), ident.span()),
        }
    }
}

struct ClosureInvoke<'binding, 'syntax, S: Target> {
    callable: &'binding ImportedCallable<S>,
    source: &'binding ClosureType,
    syntax: &'syntax ClosureSyntax<'syntax>,
}

impl<'binding, 'syntax, S: Target> ClosureInvoke<'binding, 'syntax, S> {
    fn new(
        callable: &'binding ImportedCallable<S>,
        source: &'binding ClosureType,
        syntax: &'syntax ClosureSyntax<'syntax>,
    ) -> Result<Self, Error> {
        if callable.params().len() != source.parameters.len() {
            return Err(Error::SourceSyntaxMismatch(
                "source closure parameter count does not match binding invoke parameter count",
            ));
        }
        if source.parameters.len() != syntax.parameters().len() {
            return Err(Error::SourceSyntaxMismatch(
                "closure syntax parameter count does not match source closure parameter count",
            ));
        }
        Ok(Self {
            callable,
            source,
            syntax,
        })
    }

    fn parameters(&self) -> Result<InvokeParameters, Error> {
        let tokens = self
            .callable
            .params()
            .iter()
            .zip(self.syntax.parameters().iter())
            .enumerate()
            .map(|(index, (param, rust_type))| {
                InvokeParameterInput::new(index, param.payload(), rust_type).tokens()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(InvokeParameters::from(tokens))
    }

    fn return_tokens(&self) -> Result<ForeignClosureReturnTokens<'syntax>, Error>
    where
        ForeignClosureReturnRule: RenderRule<
                S,
                ForeignClosureReturn<'binding, 'syntax, S>,
                Output = ForeignClosureReturnTokens<'syntax>,
            >,
    {
        <ForeignClosureReturnRule as RenderRule<S, ForeignClosureReturn<'binding, 'syntax, S>>>::apply(
            ForeignClosureReturnRule,
            ForeignClosureReturn::new(
                self.callable.returns().plan(),
                self.callable.error(),
                &self.source.returns,
                self.syntax.return_type(),
            ),
        )
    }
}

struct InvokeParameterInput<'binding, 'syntax, S: Target> {
    index: usize,
    payload: &'binding OutgoingParam<S>,
    rust_type: &'syntax Type,
}

impl<'binding, 'syntax, S: Target> InvokeParameterInput<'binding, 'syntax, S> {
    fn new(index: usize, payload: &'binding OutgoingParam<S>, rust_type: &'syntax Type) -> Self {
        Self {
            index,
            payload,
            rust_type,
        }
    }

    fn tokens(self) -> Result<InvokeParameterTokens, Error> {
        let argument = local::ClosureArgument::new(self.index).value();
        let rust_type = self.rust_type;
        match self.payload {
            OutgoingParam::Value(ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                ..
            }) => {
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
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
                let locals = local::ClosureArgument::new(self.index);
                let wire = locals.wire();
                let pointer = locals.pointer();
                let length = locals.length();
                let buffer = codec::EncodedValue::new(codec.root()).buffer(quote! { #argument })?;
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

struct ForeignClosureReturn<'binding, 'syntax, S: Target> {
    plan: &'binding ReturnPlan<S, IntoRust>,
    error: &'binding ErrorDecl<S, IntoRust>,
    source: &'binding ReturnDef,
    rust_type: Option<&'syntax Type>,
}

impl<'binding, 'syntax, S: Target> ForeignClosureReturn<'binding, 'syntax, S> {
    fn new(
        plan: &'binding ReturnPlan<S, IntoRust>,
        error: &'binding ErrorDecl<S, IntoRust>,
        source: &'binding ReturnDef,
        rust_type: Option<&'syntax Type>,
    ) -> Self {
        Self {
            plan,
            error,
            source,
            rust_type,
        }
    }

    fn direct_tokens(&self) -> Result<Option<ForeignClosureReturnTokens<'syntax>>, Error> {
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
                let ffi_type = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
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

    fn rust_fallible_return(&self) -> Result<RustFallibleReturn<'syntax>, Error> {
        signature::Return::new(self.source).fallible()?;
        let rust_type = self.rust_type.ok_or(Error::SourceSyntaxMismatch(
            "fallible closure invoke requires source Result return type",
        ))?;
        RustFallibleReturn::parse(rust_type).ok_or(Error::SourceSyntaxMismatch(
            "fallible closure invoke requires source Result return type",
        ))
    }
}

struct ForeignClosureReturnRule;

impl<'binding, 'syntax> RenderRule<Native, ForeignClosureReturn<'binding, 'syntax, Native>>
    for ForeignClosureReturnRule
{
    type Output = ForeignClosureReturnTokens<'syntax>;

    fn apply(
        self,
        input: ForeignClosureReturn<'binding, 'syntax, Native>,
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
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let ty = TypeRef::Primitive(*primitive);
                let ffi_type = <render::type_ref::Rule as RenderRule<Native, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?;
                Ok(ForeignClosureReturnTokens::NativeFallibleDirectPrimitive {
                    ffi_type,
                    error_type: input.rust_fallible_return()?.err,
                })
            }
            (
                ReturnPlan::DirectViaOutPointer { .. },
                ErrorDecl::EncodedViaReturnSlot {
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                Ok(ForeignClosureReturnTokens::NativeFallibleDirectPassable {
                    ok_type: result.ok,
                    error_type: result.err,
                })
            }
            (
                ReturnPlan::EncodedViaOutPointer {
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::EncodedViaReturnSlot {
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => {
                let result = input.rust_fallible_return()?;
                Ok(ForeignClosureReturnTokens::NativeFallibleEncoded {
                    ok_type: result.ok,
                    error_type: result.err,
                })
            }
            (
                ReturnPlan::Void,
                ErrorDecl::EncodedViaReturnSlot {
                    shape: native::BufferShape::Buffer,
                    ..
                },
            ) => Ok(ForeignClosureReturnTokens::NativeFallibleVoid {
                error_type: input.rust_fallible_return()?.err,
            }),
            (
                ReturnPlan::EncodedViaReturnSlot {
                    shape: native::BufferShape::Buffer,
                    ..
                },
                ErrorDecl::None(_),
            ) => {
                let rust_type = input.rust_type.ok_or(Error::SourceSyntaxMismatch(
                    "closure invoke encoded return requires source return type",
                ))?;
                Ok(ForeignClosureReturnTokens::NativeEncoded { rust_type })
            }
            (ReturnPlan::EncodedViaReturnSlot { .. }, _) => Err(Error::UnsupportedExpansion(
                "native closure invoke encoded return shape",
            )),
            _ => Err(Error::UnsupportedExpansion("closure invoke return shape")),
        }
    }
}

impl<'binding, 'syntax> RenderRule<Wasm32, ForeignClosureReturn<'binding, 'syntax, Wasm32>>
    for ForeignClosureReturnRule
{
    type Output = ForeignClosureReturnTokens<'syntax>;

    fn apply(
        self,
        input: ForeignClosureReturn<'binding, 'syntax, Wasm32>,
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
                let ffi_type = <render::type_ref::Rule as RenderRule<Wasm32, &TypeRef>>::apply(
                    render::type_ref::Rule,
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

enum ForeignClosureReturnTokens<'syntax> {
    Void,
    DirectPrimitive {
        ffi_type: TokenStream,
    },
    DirectPassable {
        rust_type: &'syntax Type,
    },
    NativeEncoded {
        rust_type: &'syntax Type,
    },
    WasmPackedString,
    NativeFallibleVoid {
        error_type: &'syntax Type,
    },
    NativeFallibleDirectPrimitive {
        ffi_type: TokenStream,
        error_type: &'syntax Type,
    },
    NativeFallibleDirectPassable {
        ok_type: &'syntax Type,
        error_type: &'syntax Type,
    },
    NativeFallibleEncoded {
        ok_type: &'syntax Type,
        error_type: &'syntax Type,
    },
    WasmFallibleVoidString,
    WasmFallibleDirectPrimitive {
        ffi_type: TokenStream,
    },
    WasmFallibleDirectPassable {
        ok_type: &'syntax Type,
    },
    WasmFalliblePackedString {
        ok_type: &'syntax Type,
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
            Self::NativeEncoded { rust_type } => quote! {
                {
                    let __boltffi_result_buf = unsafe { #call };
                    let __boltffi_result_bytes = unsafe {
                        __boltffi_result_buf.as_byte_slice()
                    };
                    ::boltffi::__private::wire::decode::<#rust_type>(__boltffi_result_bytes)
                        .expect("closure return: wire decode failed")
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
            Self::NativeFallibleVoid { error_type } => quote! {
                {
                    let __boltffi_error_buf = unsafe { #call };
                    if __boltffi_error_buf.is_empty() {
                        Ok(())
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(::boltffi::__private::wire::decode::<#error_type>(__boltffi_error_bytes)
                            .expect("closure error: wire decode failed"))
                    }
                }
            },
            Self::NativeFallibleDirectPrimitive { error_type, .. } => {
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
                        Err(::boltffi::__private::wire::decode::<#error_type>(__boltffi_error_bytes)
                            .expect("closure error: wire decode failed"))
                    }
                }
            }
            Self::NativeFallibleDirectPassable {
                ok_type,
                error_type,
            } => {
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
                        Err(::boltffi::__private::wire::decode::<#error_type>(__boltffi_error_bytes)
                            .expect("closure error: wire decode failed"))
                    }
                }
            }
            Self::NativeFallibleEncoded {
                ok_type,
                error_type,
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
                        Ok(::boltffi::__private::wire::decode::<#ok_type>(__boltffi_success_bytes)
                            .expect("closure return: wire decode failed"))
                    } else {
                        let __boltffi_error_bytes = unsafe {
                            __boltffi_error_buf.as_byte_slice()
                        };
                        Err(::boltffi::__private::wire::decode::<#error_type>(__boltffi_error_bytes)
                            .expect("closure error: wire decode failed"))
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

struct RustFallibleReturn<'syntax> {
    ok: &'syntax Type,
    err: &'syntax Type,
}

impl<'syntax> RustFallibleReturn<'syntax> {
    fn parse(ty: &'syntax Type) -> Option<Self> {
        let Type::Path(path) = ty else {
            return None;
        };
        let segment = path.path.segments.last()?;
        (segment.ident == "Result").then_some(())?;
        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        let mut types = arguments.args.iter().filter_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        });
        let ok = types.next()?;
        let err = types.next()?;
        Some(Self { ok, err })
    }
}

enum ClosureSyntax<'a> {
    ImplTrait(ClosureSignature),
    Boxed(ClosureSignature, &'a Type),
    NullableBoxed(ClosureSignature, &'a Type),
}

impl<'a> ClosureSyntax<'a> {
    fn new<S: Target>(
        ty: &'a Type,
        source: &ClosureType,
        closure: &ClosureParameter<S, IntoRust>,
    ) -> Result<Self, Error> {
        let syntax = match (closure.presence(), ty) {
            (HandlePresence::Required, Type::ImplTrait(impl_trait)) => {
                Ok(Self::ImplTrait(ClosureSignature::impl_trait(impl_trait)?))
            }
            (HandlePresence::Required, Type::Path(_)) => {
                Ok(Self::Boxed(ClosureSignature::boxed(ty)?, ty))
            }
            (HandlePresence::Nullable, Type::Path(_)) => Ok(Self::NullableBoxed(
                ClosureSignature::nullable_boxed(ty)?,
                ty,
            )),
            (HandlePresence::Required, Type::BareFn(_)) => Err(Error::UnsupportedExpansion(
                "function-pointer closure parameter",
            )),
            _ => Err(Error::SourceSyntaxMismatch(
                "closure parameter syntax does not match binding closure",
            )),
        }?;
        syntax.matches_source(source, closure.form())?;
        Ok(syntax)
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

    fn matches_source(
        &self,
        source: &ClosureType,
        form: boltffi_binding::ClosureForm,
    ) -> Result<(), Error> {
        let signature = match self {
            Self::ImplTrait(signature)
            | Self::Boxed(signature, _)
            | Self::NullableBoxed(signature, _) => signature,
        };
        signature.matches_source(source, form)
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

struct NativeBinding<'a> {
    ident: &'a Ident,
    callback: &'a Ident,
    context: &'a Ident,
    release: &'a Ident,
    owner: &'a Ident,
    rust_parameters: &'a [TokenStream],
    body: TokenStream,
    failure: &'a TokenStream,
}

struct ClosureSignature {
    form: boltffi_binding::ClosureForm,
    parameters: Vec<Type>,
    return_type: Option<Type>,
}

impl ClosureSignature {
    fn impl_trait(impl_trait: &TypeImplTrait) -> Result<Self, Error> {
        impl_trait
            .bounds
            .iter()
            .find_map(Self::bound)
            .ok_or(Error::SourceSyntaxMismatch(
                "impl closure parameter syntax does not contain a closure trait",
            ))
    }

    fn boxed(ty: &Type) -> Result<Self, Error> {
        Self::boxed_inner(ty).ok_or(Error::SourceSyntaxMismatch(
            "boxed closure parameter syntax does not match binding closure",
        ))
    }

    fn nullable_boxed(ty: &Type) -> Result<Self, Error> {
        let Type::Path(path) = ty else {
            return Err(Error::SourceSyntaxMismatch(
                "nullable closure parameter syntax does not match binding closure",
            ));
        };
        let segment = path
            .path
            .segments
            .last()
            .ok_or(Error::SourceSyntaxMismatch(
                "nullable closure parameter syntax does not match binding closure",
            ))?;
        if segment.ident != "Option" {
            return Err(Error::SourceSyntaxMismatch(
                "nullable closure parameter syntax does not match binding closure",
            ));
        }
        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return Err(Error::SourceSyntaxMismatch(
                "nullable closure parameter syntax does not match binding closure",
            ));
        };
        let inner = arguments
            .args
            .iter()
            .find_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .ok_or(Error::SourceSyntaxMismatch(
                "nullable closure parameter syntax does not match binding closure",
            ))?;
        Self::boxed_inner(inner).ok_or(Error::SourceSyntaxMismatch(
            "nullable boxed closure parameter syntax does not match binding closure",
        ))
    }

    fn boxed_inner(ty: &Type) -> Option<Self> {
        let Type::Path(path) = ty else {
            return None;
        };
        let segment = path.path.segments.last()?;
        if segment.ident != "Box" {
            return None;
        }
        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        let inner = arguments.args.iter().find_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })?;
        let Type::TraitObject(trait_object) = inner else {
            return None;
        };
        trait_object.bounds.iter().find_map(Self::bound)
    }

    fn bound(bound: &TypeParamBound) -> Option<Self> {
        let TypeParamBound::Trait(bound) = bound else {
            return None;
        };
        let segment = bound.path.segments.last()?;
        let form = match segment.ident.to_string().as_str() {
            "Fn" => boltffi_binding::ClosureForm::Fn,
            "FnMut" => boltffi_binding::ClosureForm::FnMut,
            "FnOnce" => boltffi_binding::ClosureForm::FnOnce,
            _ => return None,
        };
        let PathArguments::Parenthesized(arguments) = &segment.arguments else {
            return None;
        };
        let parameters = arguments.inputs.iter().cloned().collect();
        let return_type = match &arguments.output {
            ReturnType::Default => None,
            ReturnType::Type(_, ty) => Some(ty.as_ref().clone()),
        };
        Some(Self {
            form,
            parameters,
            return_type,
        })
    }

    fn matches_source(
        &self,
        source: &ClosureType,
        form: boltffi_binding::ClosureForm,
    ) -> Result<(), Error> {
        if self.form != form || boltffi_binding::ClosureForm::from(source.kind) != form {
            return Err(Error::SourceSyntaxMismatch(
                "closure syntax form does not match source closure form",
            ));
        }
        if self.parameters.len() != source.parameters.len() {
            return Err(Error::SourceSyntaxMismatch(
                "closure syntax parameter count does not match source closure parameter count",
            ));
        }
        match (&self.return_type, &source.returns) {
            (None, ReturnDef::Void) | (Some(_), ReturnDef::Value(_)) => Ok(()),
            _ => Err(Error::SourceSyntaxMismatch(
                "closure syntax return does not match source closure return",
            )),
        }
    }
}

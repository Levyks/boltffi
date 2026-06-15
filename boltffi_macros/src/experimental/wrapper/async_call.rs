use boltffi_binding::{
    ErrorDecl, ExecutionDecl, ExportedCallable, FunctionDecl, HandleTarget, IncomingParam,
    IntoRust, Native, NativeSymbol, ParamDecl, ParamPlan, Receive, ReturnPlan, TypeRef, Wasm32,
    native, wasm32,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Type, parse_quote};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    surface::RenderSurface,
    wrapper::{
        self, Render, export, names,
        returns::{closure, direct_vec, encoded, fallible, handle, scalar_option},
    },
};

pub struct Renderer;

pub struct Input<'expansion, 'lowered, S: RenderSurface> {
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    rust_call: export::RustCall,
    receiver: export::ReceiverTokens,
    visibility: TokenStream,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: RenderSurface> Input<'expansion, 'lowered, S> {
    pub fn new(
        function: &'lowered FunctionDecl<S>,
        source: rust_api::Callable<'lowered>,
        function_ident: Ident,
        visibility: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self::exported(
            function.symbol(),
            function.callable(),
            source,
            export::RustCall::function(function_ident),
            export::ReceiverTokens::none(),
            visibility,
            expansion,
        )
    }

    pub fn exported(
        symbol: &'lowered NativeSymbol,
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        rust_call: export::RustCall,
        receiver: export::ReceiverTokens,
        visibility: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            symbol,
            callable,
            source,
            rust_call,
            receiver,
            visibility,
            expansion,
        }
    }
}

impl<'expansion, 'lowered> Render<Native, Input<'expansion, 'lowered, Native>> for Renderer {
    type Output = TokenStream;

    fn render(self, input: Input<'expansion, 'lowered, Native>) -> Result<Self::Output, Error> {
        NativeAsync::new(input).tokens()
    }
}

impl<'expansion, 'lowered> Render<Wasm32, Input<'expansion, 'lowered, Wasm32>> for Renderer {
    type Output = TokenStream;

    fn render(self, input: Input<'expansion, 'lowered, Wasm32>) -> Result<Self::Output, Error> {
        WasmAsync::new(input).tokens()
    }
}

struct NativeAsync<'expansion, 'lowered> {
    input: Input<'expansion, 'lowered, Native>,
}

impl<'expansion, 'lowered> NativeAsync<'expansion, 'lowered> {
    fn new(input: Input<'expansion, 'lowered, Native>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<TokenStream, Error> {
        let ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle {
            poll,
            complete,
            cancel,
            free,
            panic_message,
            ..
        }) = self.input.callable.execution()
        else {
            return Err(Error::UnsupportedExpansion("native async protocol"));
        };

        AsyncExports::new(
            self.input.symbol,
            self.input.callable,
            self.input.source,
            self.input.rust_call,
            self.input.receiver,
            self.input.visibility,
            self.input.expansion,
        )?
        .tokens(NativeProtocol {
            poll,
            complete,
            cancel,
            free,
            panic_message,
        })
    }
}

struct WasmAsync<'expansion, 'lowered> {
    input: Input<'expansion, 'lowered, Wasm32>,
}

impl<'expansion, 'lowered> WasmAsync<'expansion, 'lowered> {
    fn new(input: Input<'expansion, 'lowered, Wasm32>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<TokenStream, Error> {
        let ExecutionDecl::Asynchronous(wasm32::AsyncProtocol::PollHandle {
            poll_sync,
            complete,
            cancel,
            free,
            panic_message,
            ..
        }) = self.input.callable.execution()
        else {
            return Err(Error::UnsupportedExpansion("wasm async protocol"));
        };

        AsyncExports::new(
            self.input.symbol,
            self.input.callable,
            self.input.source,
            self.input.rust_call,
            self.input.receiver,
            self.input.visibility,
            self.input.expansion,
        )?
        .tokens(WasmProtocol {
            poll_sync,
            complete,
            cancel,
            free,
            panic_message,
        })
    }
}

struct AsyncExports<'expansion, 'lowered, S: RenderSurface> {
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    source: rust_api::Callable<'lowered>,
    rust_call: export::RustCall,
    receiver: export::ReceiverTokens,
    visibility: TokenStream,
    rust_return_type: Type,
    complete: Complete,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S> AsyncExports<'expansion, 'lowered, S>
where
    S: RenderSurface,
    for<'plan> encoded::Renderer:
        Render<S, encoded::Input<'expansion, 'plan, 'lowered, S>, Output = encoded::Tokens>,
    encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
    for<'plan> closure::Write:
        Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
    direct_vec::Renderer: Render<S, direct_vec::Input, Output = wrapper::returns::Tokens>
        + Render<S, direct_vec::Empty, Output = wrapper::returns::Tokens>,
    for<'plan> fallible::Success:
        Render<S, fallible::SuccessInput<'expansion, 'plan, S>, Output = fallible::SuccessTokens>,
    for<'plan> handle::Value: Render<
            S,
            handle::ValueInput<'expansion, 'plan, S, S::HandleCarrier>,
            Output = handle::ValueTokens,
        >,
    wrapper::handle::Carrier: Render<
            S,
            wrapper::handle::CarrierInput<S::HandleCarrier>,
            Output = wrapper::handle::CarrierTokens,
        >,
    scalar_option::Renderer: Render<S, scalar_option::Input, Output = wrapper::returns::Tokens>
        + Render<S, scalar_option::Empty, Output = wrapper::returns::Tokens>,
{
    fn new(
        symbol: &'lowered NativeSymbol,
        callable: &'lowered ExportedCallable<S>,
        source: rust_api::Callable<'lowered>,
        rust_call: export::RustCall,
        receiver: export::ReceiverTokens,
        visibility: TokenStream,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Result<Self, Error> {
        let rust_return_type = source
            .returns()
            .written_type()?
            .unwrap_or_else(|| parse_quote! { () });
        let complete = Complete::new(
            symbol,
            callable,
            source.returns(),
            &rust_return_type,
            expansion,
        )?;
        Ok(Self {
            symbol,
            callable,
            source,
            rust_call,
            receiver,
            visibility,
            rust_return_type,
            complete,
            expansion,
        })
    }

    fn tokens<P>(self, protocol: P) -> Result<TokenStream, Error>
    where
        P: AsyncProtocol,
        wrapper::arguments::AsyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
    {
        let cfg = S::cfg_attr();
        let visibility = &self.visibility;
        let start_ident = format_ident!("{}", self.symbol.name().as_str());
        let rust_return_type = &self.rust_return_type;
        AsyncParameters::new(self.callable.params()).validate()?;
        if !self.receiver.writebacks().is_empty() {
            return Err(Error::UnsupportedExpansion("async receiver writeback"));
        }
        let failure = quote! {
            return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<#rust_return_type>();
        };
        let params = <wrapper::arguments::AsyncRenderer as Render<S, _>>::render(
            wrapper::arguments::AsyncRenderer,
            wrapper::arguments::Input::new(self.callable, self.source, failure, self.expansion),
        )?;
        let ffi_parameters = self
            .receiver
            .ffi_parameters()
            .iter()
            .chain(params.ffi_parameters())
            .collect::<Vec<_>>();
        let conversions = self
            .receiver
            .conversions()
            .iter()
            .chain(params.conversions())
            .collect::<Vec<_>>();
        let rust_call = self.rust_call.expression(params.rust_arguments());
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });
        let start = quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #start_ident(#(#ffi_parameters),*) -> ::boltffi::__private::RustFutureHandle {
                #(#conversions)*
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #rust_call.await
                })
            }
        };
        let poll = protocol.poll::<S>(visibility, rust_return_type);
        let complete = protocol.complete::<S>(visibility, rust_return_type, self.complete);
        let panic_message = protocol.panic_message::<S>(visibility, rust_return_type);
        let cancel = protocol.cancel::<S>(visibility, rust_return_type);
        let free = protocol.free::<S>(visibility, rust_return_type);

        Ok(quote! {
            #start
            #poll
            #complete
            #panic_message
            #cancel
            #free
        })
    }
}

struct AsyncParameters<'lowered, S: RenderSurface> {
    params: &'lowered [ParamDecl<S, IntoRust>],
}

impl<'lowered, S: RenderSurface> AsyncParameters<'lowered, S> {
    fn new(params: &'lowered [ParamDecl<S, IntoRust>]) -> Self {
        Self { params }
    }

    fn validate(&self) -> Result<(), Error> {
        self.params
            .iter()
            .try_for_each(|param| self.validate_param(param))
    }

    fn validate_param(&self, param: &ParamDecl<S, IntoRust>) -> Result<(), Error> {
        match param.payload() {
            IncomingParam::Value(ParamPlan::Direct { receive, .. })
            | IncomingParam::Value(ParamPlan::Encoded { receive, .. }) => {
                self.validate_receive(*receive)
            }
            IncomingParam::Value(ParamPlan::Handle {
                target, receive, ..
            }) => {
                self.validate_receive(*receive)?;
                match target {
                    HandleTarget::Class(_) => Ok(()),
                    HandleTarget::Callback(_) => Err(Error::UnsupportedExpansion(
                        "async callback handle parameter",
                    )),
                    _ => Err(Error::UnsupportedExpansion("async handle parameter")),
                }
            }
            IncomingParam::Value(ParamPlan::ScalarOption { .. })
            | IncomingParam::Value(ParamPlan::DirectVec { .. }) => Ok(()),
            IncomingParam::Value(_) => Err(Error::UnsupportedExpansion("async parameter shape")),
            IncomingParam::Closure(_) => {
                Err(Error::UnsupportedExpansion("async closure parameter"))
            }
        }
    }

    fn validate_receive(&self, receive: Receive) -> Result<(), Error> {
        match receive {
            Receive::ByValue => Ok(()),
            Receive::ByRef | Receive::ByMutRef => {
                Err(Error::UnsupportedExpansion("async reference parameter"))
            }
            _ => Err(Error::UnsupportedExpansion("async receive mode")),
        }
    }
}

trait AsyncProtocol {
    fn poll<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn complete<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream;
    fn panic_message<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn cancel<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn free<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream;
}

struct NativeProtocol<'symbols> {
    poll: &'symbols NativeSymbol,
    complete: &'symbols NativeSymbol,
    cancel: &'symbols NativeSymbol,
    free: &'symbols NativeSymbol,
    panic_message: &'symbols NativeSymbol,
}

impl AsyncProtocol for NativeProtocol<'_> {
    fn poll<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        let cfg = S::cfg_attr();
        let ident = symbol_ident(self.poll);
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
                callback_data: u64,
                callback: ::boltffi::__private::RustFutureContinuationCallback,
            ) {
                unsafe {
                    ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(
                        handle,
                        callback,
                        callback_data
                    )
                }
            }
        }
    }

    fn complete<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream {
        complete.tokens::<S>(visibility, symbol_ident(self.complete), rust_return_type)
    }

    fn panic_message<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        panic_message::<S>(
            visibility,
            symbol_ident(self.panic_message),
            rust_return_type,
        )
    }

    fn cancel<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        cancel::<S>(visibility, symbol_ident(self.cancel), rust_return_type)
    }

    fn free<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        free::<S>(visibility, symbol_ident(self.free), rust_return_type)
    }
}

struct WasmProtocol<'symbols> {
    poll_sync: &'symbols NativeSymbol,
    complete: &'symbols NativeSymbol,
    cancel: &'symbols NativeSymbol,
    free: &'symbols NativeSymbol,
    panic_message: &'symbols NativeSymbol,
}

impl AsyncProtocol for WasmProtocol<'_> {
    fn poll<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        let cfg = S::cfg_attr();
        let ident = symbol_ident(self.poll_sync);
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
            ) -> i32 {
                unsafe {
                    ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle)
                }
            }
        }
    }

    fn complete<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream {
        complete.tokens::<S>(visibility, symbol_ident(self.complete), rust_return_type)
    }

    fn panic_message<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        panic_message::<S>(
            visibility,
            symbol_ident(self.panic_message),
            rust_return_type,
        )
    }

    fn cancel<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        cancel::<S>(visibility, symbol_ident(self.cancel), rust_return_type)
    }

    fn free<S: RenderSurface>(
        &self,
        visibility: &TokenStream,
        rust_return_type: &Type,
    ) -> TokenStream {
        free::<S>(visibility, symbol_ident(self.free), rust_return_type)
    }
}

enum Complete {
    Plain(PlainComplete),
    Fallible(FallibleComplete),
}

struct PlainComplete {
    return_type: TokenStream,
    ok_pattern: TokenStream,
    ok_body: TokenStream,
    err_body: TokenStream,
}

impl Complete {
    fn new<'expansion, 'plan, S: RenderSurface>(
        symbol: &'plan NativeSymbol,
        callable: &'plan ExportedCallable<S>,
        source: rust_api::Return<'plan>,
        rust_return_type: &Type,
        expansion: &'expansion Expansion<'plan, S>,
    ) -> Result<Self, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'plan, 'plan, S>, Output = encoded::Tokens>,
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        closure::Write:
            Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
        direct_vec::Renderer: Render<S, direct_vec::Input, Output = wrapper::returns::Tokens>
            + Render<S, direct_vec::Empty, Output = wrapper::returns::Tokens>,
        fallible::Success: Render<
                S,
                fallible::SuccessInput<'expansion, 'plan, S>,
                Output = fallible::SuccessTokens,
            >,
        for<'handle> handle::Value: Render<
                S,
                handle::ValueInput<'expansion, 'handle, S, S::HandleCarrier>,
                Output = handle::ValueTokens,
            >,
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        scalar_option::Renderer: Render<S, scalar_option::Input, Output = wrapper::returns::Tokens>
            + Render<S, scalar_option::Empty, Output = wrapper::returns::Tokens>,
    {
        if !matches!(callable.error(), ErrorDecl::None(_)) {
            return FallibleComplete::new(
                symbol,
                callable,
                source.fallible()?,
                rust_return_type,
                expansion,
            )
            .map(Self::Fallible);
        }
        PlainComplete::new(callable, source, rust_return_type, expansion).map(Self::Plain)
    }

    fn tokens<S: RenderSurface>(
        self,
        visibility: &TokenStream,
        ident: syn::Ident,
        rust_return_type: &Type,
    ) -> TokenStream {
        match self {
            Self::Plain(complete) => complete.tokens::<S>(visibility, ident, rust_return_type),
            Self::Fallible(complete) => complete.tokens::<S>(visibility, ident),
        }
    }
}

impl PlainComplete {
    fn new<'expansion, 'plan, S: RenderSurface>(
        callable: &'plan ExportedCallable<S>,
        source: rust_api::Return<'plan>,
        rust_return_type: &Type,
        expansion: &'expansion Expansion<'plan, S>,
    ) -> Result<Self, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'plan, 'plan, S>, Output = encoded::Tokens>,
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        direct_vec::Renderer: Render<S, direct_vec::Input, Output = wrapper::returns::Tokens>
            + Render<S, direct_vec::Empty, Output = wrapper::returns::Tokens>,
        for<'handle> handle::Value: Render<
                S,
                handle::ValueInput<'expansion, 'handle, S, S::HandleCarrier>,
                Output = handle::ValueTokens,
            >,
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        scalar_option::Renderer: Render<S, scalar_option::Input, Output = wrapper::returns::Tokens>
            + Render<S, scalar_option::Empty, Output = wrapper::returns::Tokens>,
    {
        let result = names::Wrapper::new(proc_macro2::Span::call_site()).result();
        match callable.returns().plan() {
            ReturnPlan::Void => Ok(Self {
                return_type: TokenStream::new(),
                ok_pattern: quote! { _ },
                ok_body: TokenStream::new(),
                err_body: TokenStream::new(),
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let result = syn::Ident::new("result", proc_macro2::Span::call_site());
                let ty = TypeRef::Primitive(*primitive);
                let ty = <crate::experimental::wrapper::type_ref::Renderer as Render<
                    S,
                    &TypeRef,
                >>::render(
                    crate::experimental::wrapper::type_ref::Renderer, &ty
                )?;
                Ok(Self {
                    return_type: quote! { -> #ty },
                    ok_pattern: quote! { #result },
                    ok_body: quote! { #result },
                    err_body: quote! { Default::default() },
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => Ok(Self {
                return_type: quote! { -> <#rust_return_type as ::boltffi::__private::Passable>::Out },
                ok_pattern: quote! { #result },
                ok_body: quote! { ::boltffi::__private::Passable::pack(#result) },
                err_body: quote! {
                    unsafe {
                        ::core::mem::MaybeUninit::zeroed().assume_init()
                    }
                },
            }),
            ReturnPlan::EncodedViaReturnSlot { codec, shape, .. } => {
                let encoded = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Input::new(codec, *shape, result.clone(), expansion),
                )?;
                let empty = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Empty::new(*shape),
                )?;
                Ok(Self {
                    return_type: encoded.return_type().clone(),
                    ok_pattern: quote! { #result },
                    ok_body: encoded.value().clone(),
                    err_body: empty.value().clone(),
                })
            }
            ReturnPlan::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => {
                let handle_return = source.handle_return(target, *presence)?;
                let handle = <handle::Value as Render<S, _>>::render(
                    handle::Value,
                    handle::ValueInput::new(
                        expansion,
                        target,
                        *carrier,
                        *presence,
                        result.clone(),
                        handle_return,
                    ),
                )?;
                let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
                    wrapper::handle::Carrier,
                    wrapper::handle::CarrierInput::new(*carrier),
                )?;
                let ty = handle.ty();
                Ok(Self {
                    return_type: quote! { -> #ty },
                    ok_pattern: quote! { #result },
                    ok_body: handle.value().clone(),
                    err_body: carrier.zero().clone(),
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                source.scalar_option(*primitive)?;
                let optional = <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Input::new(*primitive, result.clone()),
                )?;
                let empty = <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Empty,
                )?;
                Ok(Self {
                    return_type: optional.return_type().clone(),
                    ok_pattern: quote! { #result },
                    ok_body: optional.body().clone(),
                    err_body: empty.body().clone(),
                })
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                source.direct_vec()?;
                let sequence = <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Input::new(result.clone()),
                )?;
                let empty = <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Empty,
                )?;
                Ok(Self {
                    return_type: sequence.return_type().clone(),
                    ok_pattern: quote! { #result },
                    ok_body: sequence.body().clone(),
                    err_body: empty.body().clone(),
                })
            }
            _ => Err(Error::UnsupportedExpansion("async return shape")),
        }
    }

    fn tokens<S: RenderSurface>(
        self,
        visibility: &TokenStream,
        ident: syn::Ident,
        rust_return_type: &Type,
    ) -> TokenStream {
        let cfg = S::cfg_attr();
        let Self {
            return_type,
            ok_pattern,
            ok_body,
            err_body,
        } = self;
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
                out_status: *mut ::boltffi::__private::FfiStatus,
            ) #return_type {
                match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) } {
                    Ok(#ok_pattern) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        #ok_body
                    }
                    Err(status) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = status;
                            }
                        }
                        #err_body
                    }
                }
            }
        }
    }
}

struct FallibleComplete {
    ffi_parameters: Vec<TokenStream>,
    return_type: TokenStream,
    body: TokenStream,
}

impl FallibleComplete {
    fn new<'expansion, 'plan, S: RenderSurface>(
        symbol: &'plan NativeSymbol,
        callable: &'plan ExportedCallable<S>,
        source: rust_api::Fallible<'plan>,
        rust_return_type: &Type,
        expansion: &'expansion Expansion<'plan, S>,
    ) -> Result<Self, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'plan, 'plan, S>, Output = encoded::Tokens>,
        encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        closure::Write:
            Render<S, closure::WriteInput<'expansion, 'plan, S>, Output = closure::WriteTokens>,
        fallible::Success: Render<
                S,
                fallible::SuccessInput<'expansion, 'plan, S>,
                Output = fallible::SuccessTokens,
            >,
        for<'handle> handle::Value: Render<
                S,
                handle::ValueInput<'expansion, 'handle, S, S::HandleCarrier>,
                Output = handle::ValueTokens,
            >,
    {
        let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = callable.error() else {
            return Err(Error::UnsupportedExpansion("async error channel"));
        };
        let error = names::Wrapper::new(proc_macro2::Span::call_site()).error();
        let encoded_error = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Input::new(codec, *shape, error.clone(), expansion),
        )?;
        let empty_error = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Empty::new(*shape),
        )?;
        let success = <fallible::Success as Render<S, _>>::render(
            fallible::Success,
            fallible::SuccessInput::new(
                callable.returns(),
                source,
                format_ident!("{}", symbol.name().as_str()),
                expansion,
            ),
        )?;
        let (_, ffi_parameters, success_pattern, success_body) = success.into_parts();
        let return_type = encoded_error.return_type().clone();
        let error_value = encoded_error.value();
        let empty_error_value = empty_error.value();

        Ok(Self {
            ffi_parameters,
            return_type,
            body: quote! {
                match unsafe { ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) } {
                    Ok(Ok(#success_pattern)) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        #success_body
                        #empty_error_value
                    }
                    Ok(Err(#error)) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = ::boltffi::__private::FfiStatus::OK;
                            }
                        }
                        #error_value
                    }
                    Err(status) => {
                        if !out_status.is_null() {
                            unsafe {
                                *out_status = status;
                            }
                        }
                        #empty_error_value
                    }
                }
            },
        })
    }

    fn tokens<S: RenderSurface>(self, visibility: &TokenStream, ident: syn::Ident) -> TokenStream {
        let cfg = S::cfg_attr();
        let Self {
            ffi_parameters,
            return_type,
            body,
        } = self;
        quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility unsafe extern "C" fn #ident(
                handle: ::boltffi::__private::RustFutureHandle,
                out_status: *mut ::boltffi::__private::FfiStatus
                #(, #ffi_parameters)*
            ) #return_type {
                #body
            }
        }
    }
}

fn panic_message<S: RenderSurface>(
    visibility: &TokenStream,
    ident: syn::Ident,
    rust_return_type: &Type,
) -> TokenStream {
    let cfg = S::cfg_attr();
    quote! {
        #cfg
        #[unsafe(no_mangle)]
        #visibility unsafe extern "C" fn #ident(
            handle: ::boltffi::__private::RustFutureHandle,
        ) -> ::boltffi::__private::FfiBuf {
            match unsafe { ::boltffi::__private::rustfuture::rust_future_panic_message::<#rust_return_type>(handle) } {
                Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                None => ::boltffi::__private::FfiBuf::empty(),
            }
        }
    }
}

fn cancel<S: RenderSurface>(
    visibility: &TokenStream,
    ident: syn::Ident,
    rust_return_type: &Type,
) -> TokenStream {
    let cfg = S::cfg_attr();
    quote! {
        #cfg
        #[unsafe(no_mangle)]
        #visibility unsafe extern "C" fn #ident(handle: ::boltffi::__private::RustFutureHandle) {
            unsafe {
                ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
            }
        }
    }
}

fn free<S: RenderSurface>(
    visibility: &TokenStream,
    ident: syn::Ident,
    rust_return_type: &Type,
) -> TokenStream {
    let cfg = S::cfg_attr();
    quote! {
        #cfg
        #[unsafe(no_mangle)]
        #visibility unsafe extern "C" fn #ident(handle: ::boltffi::__private::RustFutureHandle) {
            unsafe {
                ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
            }
        }
    }
}

fn symbol_ident(symbol: &NativeSymbol) -> syn::Ident {
    format_ident!("{}", symbol.name().as_str())
}

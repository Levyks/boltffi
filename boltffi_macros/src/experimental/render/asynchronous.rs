use boltffi_binding::{
    ErrorDecl, ExecutionDecl, FunctionDecl, HandleTarget, IncomingParam, IntoRust, Native,
    NativeSymbol, ParamDecl, ParamPlan, Receive, ReturnPlan, TypeRef, Wasm32, native, wasm32,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ItemFn, PatType, ReturnType, Type};

use crate::experimental::{
    error::Error,
    render::{
        self, Rule as RenderRule,
        callable::signature,
        local,
        returns::{direct_vec, encoded, fallible, handle, scalar_option},
    },
    target::Target,
};

pub struct Rule;

pub struct Input<'binding, 'syntax, S: Target> {
    function: &'binding FunctionDecl<S>,
    source: signature::Callable<'binding>,
    syntax: &'syntax ItemFn,
}

impl<'binding, 'syntax, S: Target> Input<'binding, 'syntax, S> {
    pub fn new(
        function: &'binding FunctionDecl<S>,
        source: signature::Callable<'binding>,
        syntax: &'syntax ItemFn,
    ) -> Self {
        Self {
            function,
            source,
            syntax,
        }
    }
}

impl<'binding, 'syntax> RenderRule<Native, Input<'binding, 'syntax, Native>> for Rule {
    type Output = TokenStream;

    fn apply(self, input: Input<'binding, 'syntax, Native>) -> Result<Self::Output, Error> {
        NativeAsync::new(input).tokens()
    }
}

impl<'binding, 'syntax> RenderRule<Wasm32, Input<'binding, 'syntax, Wasm32>> for Rule {
    type Output = TokenStream;

    fn apply(self, input: Input<'binding, 'syntax, Wasm32>) -> Result<Self::Output, Error> {
        WasmAsync::new(input).tokens()
    }
}

struct NativeAsync<'binding, 'syntax> {
    input: Input<'binding, 'syntax, Native>,
}

impl<'binding, 'syntax> NativeAsync<'binding, 'syntax> {
    fn new(input: Input<'binding, 'syntax, Native>) -> Self {
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
        }) = self.input.function.callable().execution()
        else {
            return Err(Error::UnsupportedExpansion("native async protocol"));
        };

        AsyncExports::new(self.input.function, self.input.source, self.input.syntax)?.tokens(
            NativeProtocol {
                poll,
                complete,
                cancel,
                free,
                panic_message,
            },
        )
    }
}

struct WasmAsync<'binding, 'syntax> {
    input: Input<'binding, 'syntax, Wasm32>,
}

impl<'binding, 'syntax> WasmAsync<'binding, 'syntax> {
    fn new(input: Input<'binding, 'syntax, Wasm32>) -> Self {
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
        }) = self.input.function.callable().execution()
        else {
            return Err(Error::UnsupportedExpansion("wasm async protocol"));
        };

        AsyncExports::new(self.input.function, self.input.source, self.input.syntax)?.tokens(
            WasmProtocol {
                poll_sync,
                complete,
                cancel,
                free,
                panic_message,
            },
        )
    }
}

struct AsyncExports<'binding, 'syntax, S: Target> {
    function: &'binding FunctionDecl<S>,
    source: signature::Callable<'binding>,
    syntax: &'syntax ItemFn,
    rust_return_type: Type,
    complete: Complete,
}

impl<'binding, 'syntax, S> AsyncExports<'binding, 'syntax, S>
where
    S: Target,
    for<'plan> encoded::Rule: RenderRule<S, encoded::Input<'plan, S>, Output = encoded::Tokens>,
    encoded::Rule: RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
    direct_vec::Rule: RenderRule<S, direct_vec::Input, Output = render::returns::Tokens>
        + RenderRule<S, direct_vec::Empty, Output = render::returns::Tokens>,
    fallible::Success: for<'plan> RenderRule<S, fallible::SuccessInput<'plan, S>, Output = fallible::SuccessTokens>,
    for<'plan> handle::Value:
        RenderRule<S, handle::ValueInput<'plan, S::HandleCarrier>, Output = handle::ValueTokens>,
    render::handle::Carrier: RenderRule<
            S,
            render::handle::CarrierInput<S::HandleCarrier>,
            Output = render::handle::CarrierTokens,
        >,
    scalar_option::Rule: RenderRule<S, scalar_option::Input, Output = render::returns::Tokens>
        + RenderRule<S, scalar_option::Empty, Output = render::returns::Tokens>,
{
    fn new(
        function: &'binding FunctionDecl<S>,
        source: signature::Callable<'binding>,
        syntax: &'syntax ItemFn,
    ) -> Result<Self, Error> {
        let rust_return_type = syntax_return_type(syntax);
        let complete = Complete::new(function, source.returns(), &rust_return_type)?;
        Ok(Self {
            function,
            source,
            syntax,
            rust_return_type,
            complete,
        })
    }

    fn tokens<P>(self, protocol: P) -> Result<TokenStream, Error>
    where
        P: AsyncProtocol,
        for<'params> render::callable::Parameters: RenderRule<
                S,
                render::callable::Input<'binding, 'params, 'syntax, S>,
                Output = render::callable::Tokens,
            >,
    {
        let cfg = S::cfg_attr();
        let visibility = &self.syntax.vis;
        let start_ident = format_ident!("{}", self.function.symbol().name().as_str());
        let function_ident = &self.syntax.sig.ident;
        let rust_return_type = &self.rust_return_type;
        let syntax_params = syntax_params(self.syntax)?;
        AsyncParameters::new(self.function.callable().params()).validate()?;
        let failure = quote! {
            return ::boltffi::__private::rustfuture::rust_future_invalid_arg::<#rust_return_type>();
        };
        let params = <render::callable::Parameters as RenderRule<S, _>>::apply(
            render::callable::Parameters,
            render::callable::Input::new(
                self.function.callable(),
                self.source,
                &syntax_params,
                failure,
            ),
        )?;
        let ffi_parameters = params.ffi_parameters();
        let conversions = params.conversions();
        let arguments = params.arguments();
        let safety = (!ffi_parameters.is_empty()).then(|| quote! { unsafe });
        let start = quote! {
            #cfg
            #[unsafe(no_mangle)]
            #visibility #safety extern "C" fn #start_ident(#(#ffi_parameters),*) -> ::boltffi::__private::RustFutureHandle {
                #(#conversions)*
                ::boltffi::__private::rustfuture::rust_future_new(async move {
                    #function_ident(#(#arguments),*).await
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

struct AsyncParameters<'binding, S: Target> {
    params: &'binding [ParamDecl<S, IntoRust>],
}

impl<'binding, S: Target> AsyncParameters<'binding, S> {
    fn new(params: &'binding [ParamDecl<S, IntoRust>]) -> Self {
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
    fn poll<S: Target>(&self, visibility: &syn::Visibility, rust_return_type: &Type)
    -> TokenStream;
    fn complete<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream;
    fn panic_message<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn cancel<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream;
    fn free<S: Target>(&self, visibility: &syn::Visibility, rust_return_type: &Type)
    -> TokenStream;
}

struct NativeProtocol<'a> {
    poll: &'a NativeSymbol,
    complete: &'a NativeSymbol,
    cancel: &'a NativeSymbol,
    free: &'a NativeSymbol,
    panic_message: &'a NativeSymbol,
}

impl AsyncProtocol for NativeProtocol<'_> {
    fn poll<S: Target>(
        &self,
        visibility: &syn::Visibility,
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
                ::boltffi::__private::rustfuture::rust_future_poll::<#rust_return_type>(
                    handle,
                    callback,
                    callback_data
                )
            }
        }
    }

    fn complete<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream {
        complete.tokens::<S>(visibility, symbol_ident(self.complete), rust_return_type)
    }

    fn panic_message<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream {
        panic_message::<S>(
            visibility,
            symbol_ident(self.panic_message),
            rust_return_type,
        )
    }

    fn cancel<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream {
        cancel::<S>(visibility, symbol_ident(self.cancel), rust_return_type)
    }

    fn free<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream {
        free::<S>(visibility, symbol_ident(self.free), rust_return_type)
    }
}

struct WasmProtocol<'a> {
    poll_sync: &'a NativeSymbol,
    complete: &'a NativeSymbol,
    cancel: &'a NativeSymbol,
    free: &'a NativeSymbol,
    panic_message: &'a NativeSymbol,
}

impl AsyncProtocol for WasmProtocol<'_> {
    fn poll<S: Target>(
        &self,
        visibility: &syn::Visibility,
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
                ::boltffi::__private::rust_future_poll_sync::<#rust_return_type>(handle)
            }
        }
    }

    fn complete<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
        complete: Complete,
    ) -> TokenStream {
        complete.tokens::<S>(visibility, symbol_ident(self.complete), rust_return_type)
    }

    fn panic_message<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream {
        panic_message::<S>(
            visibility,
            symbol_ident(self.panic_message),
            rust_return_type,
        )
    }

    fn cancel<S: Target>(
        &self,
        visibility: &syn::Visibility,
        rust_return_type: &Type,
    ) -> TokenStream {
        cancel::<S>(visibility, symbol_ident(self.cancel), rust_return_type)
    }

    fn free<S: Target>(
        &self,
        visibility: &syn::Visibility,
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
    fn new<S: Target>(
        function: &FunctionDecl<S>,
        source: signature::Return<'_>,
        rust_return_type: &Type,
    ) -> Result<Self, Error>
    where
        for<'plan> encoded::Rule: RenderRule<S, encoded::Input<'plan, S>, Output = encoded::Tokens>,
        encoded::Rule: RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
        direct_vec::Rule: RenderRule<S, direct_vec::Input, Output = render::returns::Tokens>
            + RenderRule<S, direct_vec::Empty, Output = render::returns::Tokens>,
        fallible::Success: for<'plan> RenderRule<
                S,
                fallible::SuccessInput<'plan, S>,
                Output = fallible::SuccessTokens,
            >,
        for<'plan> handle::Value: RenderRule<S, handle::ValueInput<'plan, S::HandleCarrier>, Output = handle::ValueTokens>,
        render::handle::Carrier: RenderRule<
                S,
                render::handle::CarrierInput<S::HandleCarrier>,
                Output = render::handle::CarrierTokens,
            >,
        scalar_option::Rule: RenderRule<S, scalar_option::Input, Output = render::returns::Tokens>
            + RenderRule<S, scalar_option::Empty, Output = render::returns::Tokens>,
    {
        if !matches!(function.callable().error(), ErrorDecl::None(_)) {
            return FallibleComplete::new(function, source.fallible()?, rust_return_type)
                .map(Self::Fallible);
        }
        PlainComplete::new(function, source, rust_return_type).map(Self::Plain)
    }

    fn tokens<S: Target>(
        self,
        visibility: &syn::Visibility,
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
    fn new<S: Target>(
        function: &FunctionDecl<S>,
        source: signature::Return<'_>,
        rust_return_type: &Type,
    ) -> Result<Self, Error>
    where
        for<'plan> encoded::Rule: RenderRule<S, encoded::Input<'plan, S>, Output = encoded::Tokens>,
        encoded::Rule: RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
        direct_vec::Rule: RenderRule<S, direct_vec::Input, Output = render::returns::Tokens>
            + RenderRule<S, direct_vec::Empty, Output = render::returns::Tokens>,
        for<'plan> handle::Value: RenderRule<S, handle::ValueInput<'plan, S::HandleCarrier>, Output = handle::ValueTokens>,
        render::handle::Carrier: RenderRule<
                S,
                render::handle::CarrierInput<S::HandleCarrier>,
                Output = render::handle::CarrierTokens,
            >,
        scalar_option::Rule: RenderRule<S, scalar_option::Input, Output = render::returns::Tokens>
            + RenderRule<S, scalar_option::Empty, Output = render::returns::Tokens>,
    {
        let result = local::Wrapper::new(proc_macro2::Span::call_site()).result();
        match function.callable().returns().plan() {
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
                let ty = <crate::experimental::render::type_ref::Rule as RenderRule<
                    S,
                    &TypeRef,
                >>::apply(
                    crate::experimental::render::type_ref::Rule, &ty
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
                let encoded = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
                    encoded::Input::new(codec, *shape, result.clone()),
                )?;
                let empty = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
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
                source.handle(target, *presence)?;
                let handle = <handle::Value as RenderRule<S, _>>::apply(
                    handle::Value,
                    handle::ValueInput::new(target, *carrier, *presence, result.clone()),
                )?;
                let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
                    render::handle::Carrier,
                    render::handle::CarrierInput::new(*carrier),
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
                let optional = <scalar_option::Rule as RenderRule<S, _>>::apply(
                    scalar_option::Rule,
                    scalar_option::Input::new(*primitive, result.clone()),
                )?;
                let empty = <scalar_option::Rule as RenderRule<S, _>>::apply(
                    scalar_option::Rule,
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
                let sequence = <direct_vec::Rule as RenderRule<S, _>>::apply(
                    direct_vec::Rule,
                    direct_vec::Input::new(result.clone()),
                )?;
                let empty = <direct_vec::Rule as RenderRule<S, _>>::apply(
                    direct_vec::Rule,
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

    fn tokens<S: Target>(
        self,
        visibility: &syn::Visibility,
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
                match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Ok(#ok_pattern) => {
                        if !out_status.is_null() {
                            *out_status = ::boltffi::__private::FfiStatus::OK;
                        }
                        #ok_body
                    }
                    Err(status) => {
                        if !out_status.is_null() {
                            *out_status = status;
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
    fn new<S: Target>(
        function: &FunctionDecl<S>,
        source: signature::Fallible<'_>,
        rust_return_type: &Type,
    ) -> Result<Self, Error>
    where
        for<'plan> encoded::Rule: RenderRule<S, encoded::Input<'plan, S>, Output = encoded::Tokens>,
        encoded::Rule: RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
        fallible::Success: for<'plan> RenderRule<
                S,
                fallible::SuccessInput<'plan, S>,
                Output = fallible::SuccessTokens,
            >,
        for<'plan> handle::Value: RenderRule<S, handle::ValueInput<'plan, S::HandleCarrier>, Output = handle::ValueTokens>,
    {
        let ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } = function.callable().error()
        else {
            return Err(Error::UnsupportedExpansion("async error channel"));
        };
        let error = local::Wrapper::new(proc_macro2::Span::call_site()).error();
        let encoded_error = <encoded::Rule as RenderRule<S, _>>::apply(
            encoded::Rule,
            encoded::Input::new(codec, *shape, error.clone()),
        )?;
        let empty_error =
            <encoded::Rule as RenderRule<S, _>>::apply(encoded::Rule, encoded::Empty::new(*shape))?;
        let success = <fallible::Success as RenderRule<S, _>>::apply(
            fallible::Success,
            fallible::SuccessInput::new(
                function.callable().returns(),
                source,
                Some(rust_return_type.clone()),
                format_ident!("{}", function.symbol().name().as_str()),
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
                match ::boltffi::__private::rustfuture::rust_future_complete::<#rust_return_type>(handle) {
                    Ok(Ok(#success_pattern)) => {
                        if !out_status.is_null() {
                            *out_status = ::boltffi::__private::FfiStatus::OK;
                        }
                        #success_body
                        #empty_error_value
                    }
                    Ok(Err(#error)) => {
                        if !out_status.is_null() {
                            *out_status = ::boltffi::__private::FfiStatus::OK;
                        }
                        #error_value
                    }
                    Err(status) => {
                        if !out_status.is_null() {
                            *out_status = status;
                        }
                        #empty_error_value
                    }
                }
            },
        })
    }

    fn tokens<S: Target>(self, visibility: &syn::Visibility, ident: syn::Ident) -> TokenStream {
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

fn panic_message<S: Target>(
    visibility: &syn::Visibility,
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
            match ::boltffi::__private::rustfuture::rust_future_panic_message::<#rust_return_type>(handle) {
                Some(message) => ::boltffi::__private::FfiBuf::wire_encode(&message),
                None => ::boltffi::__private::FfiBuf::empty(),
            }
        }
    }
}

fn cancel<S: Target>(
    visibility: &syn::Visibility,
    ident: syn::Ident,
    rust_return_type: &Type,
) -> TokenStream {
    let cfg = S::cfg_attr();
    quote! {
        #cfg
        #[unsafe(no_mangle)]
        #visibility unsafe extern "C" fn #ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_cancel::<#rust_return_type>(handle)
        }
    }
}

fn free<S: Target>(
    visibility: &syn::Visibility,
    ident: syn::Ident,
    rust_return_type: &Type,
) -> TokenStream {
    let cfg = S::cfg_attr();
    quote! {
        #cfg
        #[unsafe(no_mangle)]
        #visibility unsafe extern "C" fn #ident(handle: ::boltffi::__private::RustFutureHandle) {
            ::boltffi::__private::rustfuture::rust_future_free::<#rust_return_type>(handle)
        }
    }
}

fn symbol_ident(symbol: &NativeSymbol) -> syn::Ident {
    format_ident!("{}", symbol.name().as_str())
}

fn syntax_return_type(syntax: &ItemFn) -> Type {
    match &syntax.sig.output {
        ReturnType::Default => syn::parse_quote! { () },
        ReturnType::Type(_, ty) => ty.as_ref().clone(),
    }
}

fn syntax_params(syntax: &ItemFn) -> Result<Vec<&PatType>, Error> {
    syntax
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            FnArg::Typed(typed) => Ok(typed),
            FnArg::Receiver(_) => Err(Error::SourceSyntaxMismatch(
                "function syntax unexpectedly contains a receiver",
            )),
        })
        .collect()
}

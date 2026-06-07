use boltffi_binding::{
    CallbackLocalHandle, HandlePresence, HandleTarget, Native, Wasm32, native, wasm32,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse_str;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{self, Render},
};

pub struct Value;
pub struct Failure;

pub struct ValueInput<'context, 'a, S: Target, C> {
    expansion: &'context Expansion<'a, S>,
    target: &'a HandleTarget,
    carrier: C,
    presence: HandlePresence,
    value: syn::Ident,
    handle_return: rust_api::HandleReturn,
}

impl<'context, 'a, S: Target, C> ValueInput<'context, 'a, S, C> {
    pub fn new(
        expansion: &'context Expansion<'a, S>,
        target: &'a HandleTarget,
        carrier: C,
        presence: HandlePresence,
        value: syn::Ident,
        handle_return: rust_api::HandleReturn,
    ) -> Self {
        Self {
            expansion,
            target,
            carrier,
            presence,
            value,
            handle_return,
        }
    }
}

pub struct FailureInput<C> {
    target: HandleTarget,
    carrier: C,
}

impl<C> FailureInput<C> {
    pub fn new(target: HandleTarget, carrier: C) -> Self {
        Self { target, carrier }
    }
}

pub struct ValueTokens {
    ty: TokenStream,
    value: TokenStream,
}

impl ValueTokens {
    pub fn ty(&self) -> &TokenStream {
        &self.ty
    }

    pub fn value(&self) -> &TokenStream {
        &self.value
    }
}

impl<'context, 'a> Render<Native, ValueInput<'context, 'a, Native, native::HandleCarrier>>
    for Value
{
    type Output = ValueTokens;

    fn render(
        self,
        input: ValueInput<'context, 'a, Native, native::HandleCarrier>,
    ) -> Result<Self::Output, Error> {
        NativeReturn::new(input).tokens()
    }
}

impl<'context, 'a> Render<Wasm32, ValueInput<'context, 'a, Wasm32, wasm32::HandleCarrier>>
    for Value
{
    type Output = ValueTokens;

    fn render(
        self,
        input: ValueInput<'context, 'a, Wasm32, wasm32::HandleCarrier>,
    ) -> Result<Self::Output, Error> {
        WasmReturn::new(input).tokens()
    }
}

impl Render<Native, FailureInput<native::HandleCarrier>> for Failure {
    type Output = TokenStream;

    fn render(self, input: FailureInput<native::HandleCarrier>) -> Result<Self::Output, Error> {
        HandleFailure::new(input).tokens::<Native>()
    }
}

impl Render<Wasm32, FailureInput<wasm32::HandleCarrier>> for Failure {
    type Output = TokenStream;

    fn render(self, input: FailureInput<wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        HandleFailure::new(input).tokens::<Wasm32>()
    }
}

struct NativeReturn<'context, 'a> {
    input: ValueInput<'context, 'a, Native, native::HandleCarrier>,
}

impl<'context, 'a> NativeReturn<'context, 'a> {
    fn new(input: ValueInput<'context, 'a, Native, native::HandleCarrier>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<ValueTokens, Error> {
        let carrier = <wrapper::handle::Carrier as Render<Native, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.input.carrier),
        )?;
        let ty = carrier.ty().clone();
        let zero = carrier.zero();
        let value = match self.input.handle_return {
            rust_api::HandleReturn::Class => self.class_value(&ty, zero)?,
            rust_api::HandleReturn::Callback(ref callback) => {
                self.callback_value(callback, zero)?
            }
        };

        Ok(ValueTokens { ty, value })
    }

    fn class_value(&self, ty: &TokenStream, zero: &TokenStream) -> Result<TokenStream, Error> {
        if !matches!(self.input.target, HandleTarget::Class(_)) {
            return Err(Error::UnsupportedExpansion("non-class handle return"));
        }
        let value = &self.input.value;
        match self.input.presence {
            HandlePresence::Required => Ok(quote! {
                Box::into_raw(Box::new(#value)) as usize as #ty
            }),
            HandlePresence::Nullable => Ok(quote! {
                match #value {
                    Some(__boltffi_value) => {
                        Box::into_raw(Box::new(__boltffi_value)) as usize as #ty
                    }
                    None => #zero,
                }
            }),
            _ => Err(Error::UnsupportedExpansion("unknown class handle presence")),
        }
    }

    fn callback_value(
        &self,
        callback: &rust_api::CallbackReturn,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let HandleTarget::Callback(id) = self.input.target else {
            return Err(Error::UnsupportedExpansion("non-callback handle return"));
        };
        let declaration = self.input.expansion.callback(*id)?;
        let local_handle = LocalHandlePath::new(declaration.local_handle()).tokens()?;
        let value = &self.input.value;
        Ok(match (callback.form(), callback.presence()) {
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Required) => quote! {
                #local_handle(::std::sync::Arc::from(#value))
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Required) => quote! {
                #local_handle(#value)
            },
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| {
                        #local_handle(::std::sync::Arc::from(__boltffi_callback))
                    })
                    .unwrap_or(#zero)
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(#local_handle)
                    .unwrap_or(#zero)
            },
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown callback handle presence",
                ));
            }
        })
    }
}

struct WasmReturn<'context, 'a> {
    input: ValueInput<'context, 'a, Wasm32, wasm32::HandleCarrier>,
}

impl<'context, 'a> WasmReturn<'context, 'a> {
    fn new(input: ValueInput<'context, 'a, Wasm32, wasm32::HandleCarrier>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<ValueTokens, Error> {
        let carrier = <wrapper::handle::Carrier as Render<Wasm32, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.input.carrier),
        )?;
        let ty = carrier.ty().clone();
        let zero = carrier.zero();
        let value = match self.input.handle_return {
            rust_api::HandleReturn::Class => self.class_value(&ty, zero)?,
            rust_api::HandleReturn::Callback(ref callback) => {
                self.callback_value(callback, zero)?
            }
        };

        Ok(ValueTokens { ty, value })
    }

    fn class_value(&self, ty: &TokenStream, zero: &TokenStream) -> Result<TokenStream, Error> {
        if !matches!(self.input.target, HandleTarget::Class(_)) {
            return Err(Error::UnsupportedExpansion("non-class handle return"));
        }
        let value = &self.input.value;
        match self.input.presence {
            HandlePresence::Required => Ok(quote! {
                Box::into_raw(Box::new(#value)) as usize as #ty
            }),
            HandlePresence::Nullable => Ok(quote! {
                match #value {
                    Some(__boltffi_value) => {
                        Box::into_raw(Box::new(__boltffi_value)) as usize as #ty
                    }
                    None => #zero,
                }
            }),
            _ => Err(Error::UnsupportedExpansion("unknown class handle presence")),
        }
    }

    fn callback_value(
        &self,
        callback: &rust_api::CallbackReturn,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let HandleTarget::Callback(id) = self.input.target else {
            return Err(Error::UnsupportedExpansion("non-callback handle return"));
        };
        let declaration = self.input.expansion.callback(*id)?;
        let local_handle = LocalHandlePath::new(declaration.local_handle()).tokens()?;
        let value = &self.input.value;
        Ok(match (callback.form(), callback.presence()) {
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Required) => quote! {
                #local_handle(::std::sync::Arc::from(#value)).handle() as u32
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Required) => quote! {
                #local_handle(#value).handle() as u32
            },
            (rust_api::CallbackCarrier::BoxedDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| {
                        #local_handle(::std::sync::Arc::from(__boltffi_callback)).handle() as u32
                    })
                    .unwrap_or(#zero)
            },
            (rust_api::CallbackCarrier::ArcDyn, HandlePresence::Nullable) => quote! {
                #value
                    .map(|__boltffi_callback| #local_handle(__boltffi_callback).handle() as u32)
                    .unwrap_or(#zero)
            },
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown callback handle presence",
                ));
            }
        })
    }
}

struct LocalHandlePath<'a> {
    handle: &'a CallbackLocalHandle,
}

impl<'a> LocalHandlePath<'a> {
    fn new(handle: &'a CallbackLocalHandle) -> Self {
        Self { handle }
    }

    fn tokens(self) -> Result<TokenStream, Error> {
        let suffix = self
            .handle
            .segments()
            .iter()
            .map(|segment| segment.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let path = parse_str::<syn::Path>(&format!("crate::{suffix}"))
            .map_err(|_| Error::SourceSyntaxMismatch("callback local handle path is not Rust"))?;
        Ok(quote! { #path })
    }
}

struct HandleFailure<C> {
    input: FailureInput<C>,
}

impl<C> HandleFailure<C> {
    fn new(input: FailureInput<C>) -> Self {
        Self { input }
    }

    fn tokens<S>(self) -> Result<TokenStream, Error>
    where
        C: Copy,
        S: crate::experimental::target::Target<HandleCarrier = C>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        if !matches!(
            self.input.target,
            HandleTarget::Class(_) | HandleTarget::Callback(_)
        ) {
            return Err(Error::UnsupportedExpansion("unknown handle return failure"));
        }
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.input.carrier),
        )?;
        let zero = carrier.zero();
        Ok(quote! { return #zero; })
    }
}

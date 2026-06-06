use boltffi_binding::{HandlePresence, HandleTarget, Native, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
};

pub struct Value;
pub struct Failure;

pub struct ValueInput<'a, C> {
    target: &'a HandleTarget,
    carrier: C,
    presence: HandlePresence,
    value: syn::Ident,
}

impl<'a, C> ValueInput<'a, C> {
    pub fn new(
        target: &'a HandleTarget,
        carrier: C,
        presence: HandlePresence,
        value: syn::Ident,
    ) -> Self {
        Self {
            target,
            carrier,
            presence,
            value,
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

impl<'a> RenderRule<Native, ValueInput<'a, native::HandleCarrier>> for Value {
    type Output = ValueTokens;

    fn apply(self, input: ValueInput<'a, native::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassReturn::new(input).tokens::<Native>()
    }
}

impl<'a> RenderRule<Wasm32, ValueInput<'a, wasm32::HandleCarrier>> for Value {
    type Output = ValueTokens;

    fn apply(self, input: ValueInput<'a, wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassReturn::new(input).tokens::<Wasm32>()
    }
}

impl RenderRule<Native, FailureInput<native::HandleCarrier>> for Failure {
    type Output = TokenStream;

    fn apply(self, input: FailureInput<native::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassFailure::new(input).tokens::<Native>()
    }
}

impl RenderRule<Wasm32, FailureInput<wasm32::HandleCarrier>> for Failure {
    type Output = TokenStream;

    fn apply(self, input: FailureInput<wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassFailure::new(input).tokens::<Wasm32>()
    }
}

struct ClassReturn<'a, C> {
    input: ValueInput<'a, C>,
}

impl<'a, C> ClassReturn<'a, C> {
    fn new(input: ValueInput<'a, C>) -> Self {
        Self { input }
    }

    fn tokens<S>(self) -> Result<ValueTokens, Error>
    where
        C: Copy,
        S: crate::experimental::target::Target<HandleCarrier = C>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
    {
        if !matches!(self.input.target, HandleTarget::Class(_)) {
            return Err(Error::UnsupportedExpansion("non-class handle return"));
        }

        let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
            render::handle::Carrier,
            render::handle::CarrierInput::new(self.input.carrier),
        )?;
        let ty = carrier.ty().clone();
        let zero = carrier.zero();
        let value = self.value(&ty, zero)?;

        Ok(ValueTokens { ty, value })
    }

    fn value(&self, ty: &TokenStream, zero: &TokenStream) -> Result<TokenStream, Error> {
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
}

struct ClassFailure<C> {
    input: FailureInput<C>,
}

impl<C> ClassFailure<C> {
    fn new(input: FailureInput<C>) -> Self {
        Self { input }
    }

    fn tokens<S>(self) -> Result<TokenStream, Error>
    where
        C: Copy,
        S: crate::experimental::target::Target<HandleCarrier = C>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
    {
        if !matches!(self.input.target, HandleTarget::Class(_)) {
            return Err(Error::UnsupportedExpansion(
                "non-class handle return failure",
            ));
        }
        let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
            render::handle::Carrier,
            render::handle::CarrierInput::new(self.input.carrier),
        )?;
        let zero = carrier.zero();
        Ok(quote! { return #zero; })
    }
}

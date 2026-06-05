use boltffi_binding::{Native, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{error::Error, render::Rule as RenderRule};

pub struct Carrier;

pub struct CarrierInput<C> {
    carrier: C,
}

impl<C> CarrierInput<C> {
    pub fn new(carrier: C) -> Self {
        Self { carrier }
    }
}

pub struct CarrierTokens {
    ty: TokenStream,
    zero: TokenStream,
}

impl CarrierTokens {
    pub fn ty(&self) -> &TokenStream {
        &self.ty
    }

    pub fn zero(&self) -> &TokenStream {
        &self.zero
    }
}

impl RenderRule<Native, CarrierInput<native::HandleCarrier>> for Carrier {
    type Output = CarrierTokens;

    fn apply(self, input: CarrierInput<native::HandleCarrier>) -> Result<Self::Output, Error> {
        match input.carrier {
            native::HandleCarrier::U64 => Ok(CarrierTokens {
                ty: quote! { u64 },
                zero: quote! { 0 },
            }),
            native::HandleCarrier::USize => Ok(CarrierTokens {
                ty: quote! { usize },
                zero: quote! { 0 },
            }),
            native::HandleCarrier::CallbackHandle => Ok(CarrierTokens {
                ty: quote! { ::boltffi::__private::CallbackHandle },
                zero: quote! { ::boltffi::__private::CallbackHandle::NULL },
            }),
            _ => Err(Error::UnsupportedExpansion("unknown native handle carrier")),
        }
    }
}

impl RenderRule<Wasm32, CarrierInput<wasm32::HandleCarrier>> for Carrier {
    type Output = CarrierTokens;

    fn apply(self, input: CarrierInput<wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        match input.carrier {
            wasm32::HandleCarrier::U32 => Ok(CarrierTokens {
                ty: quote! { u32 },
                zero: quote! { 0 },
            }),
            _ => Err(Error::UnsupportedExpansion("unknown wasm handle carrier")),
        }
    }
}

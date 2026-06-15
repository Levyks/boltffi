use boltffi_binding::{HandlePresence, HandleTarget, Native, Receive, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::experimental::{
    error::Error,
    rust_api,
    wrapper::{self, Render, names},
};

use super::Tokens;

pub struct Renderer;
struct CallbackCarrier;

pub struct Plan<'lowered, C> {
    target: &'lowered HandleTarget,
    carrier: C,
    presence: HandlePresence,
    receive: Receive,
}

pub struct Input<'lowered, C> {
    plan: Plan<'lowered, C>,
    source: rust_api::Parameter<'lowered>,
    ident: Ident,
    failure: TokenStream,
}

struct CallbackHandleInput<'ident> {
    ident: &'ident Ident,
}

impl<'ident> CallbackHandleInput<'ident> {
    fn new(ident: &'ident Ident) -> Self {
        Self { ident }
    }
}

impl<'ident> Render<Native, CallbackHandleInput<'ident>> for CallbackCarrier {
    type Output = TokenStream;

    fn render(self, input: CallbackHandleInput<'ident>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { #ident })
    }
}

impl<'ident> Render<Wasm32, CallbackHandleInput<'ident>> for CallbackCarrier {
    type Output = TokenStream;

    fn render(self, input: CallbackHandleInput<'ident>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { ::boltffi::__private::CallbackHandle::from_wasm_handle(#ident) })
    }
}

impl<'lowered, C> Plan<'lowered, C> {
    pub fn new(
        target: &'lowered HandleTarget,
        carrier: C,
        presence: HandlePresence,
        receive: Receive,
    ) -> Self {
        Self {
            target,
            carrier,
            presence,
            receive,
        }
    }
}

impl<'lowered, C> Input<'lowered, C> {
    pub fn new(
        plan: Plan<'lowered, C>,
        source: rust_api::Parameter<'lowered>,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            plan,
            source,
            ident,
            failure,
        }
    }
}

impl<'lowered> Render<Native, Input<'lowered, native::HandleCarrier>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'lowered, native::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassParam::new(input).tokens::<Native>()
    }
}

impl<'lowered> Render<Wasm32, Input<'lowered, wasm32::HandleCarrier>> for Renderer {
    type Output = Tokens;

    fn render(self, input: Input<'lowered, wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassParam::new(input).tokens::<Wasm32>()
    }
}

struct ClassParam<'lowered, C> {
    input: Input<'lowered, C>,
}

impl<'lowered, C> ClassParam<'lowered, C> {
    fn new(input: Input<'lowered, C>) -> Self {
        Self { input }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: crate::experimental::surface::RenderSurface<HandleCarrier = C>,
        for<'ident> CallbackCarrier: Render<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        match self.input.plan.target {
            HandleTarget::Class(_) => self.class_tokens::<S>(),
            HandleTarget::Callback(_) => self.callback_tokens::<S>(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown handle parameter target",
            )),
        }
    }

    fn class_tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: crate::experimental::surface::RenderSurface<HandleCarrier = C>,
        for<'ident> CallbackCarrier: Render<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.input.plan.carrier),
        )?;
        let ident = &self.input.ident;
        let ffi_type = carrier.ty();
        let class = self.input.source.class_handle(
            self.input.plan.target,
            self.input.plan.presence,
            self.input.plan.receive,
        )?;
        let conversion = self.conversion(&class, carrier.zero())?;

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type.clone()],
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn callback_tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: crate::experimental::surface::RenderSurface<HandleCarrier = C>,
        for<'ident> CallbackCarrier: Render<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        wrapper::handle::Carrier:
            Render<S, wrapper::handle::CarrierInput<C>, Output = wrapper::handle::CarrierTokens>,
    {
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.input.plan.carrier),
        )?;
        let ident = &self.input.ident;
        let ffi_type = carrier.ty();
        let callback = self
            .input
            .source
            .callback_object(self.input.plan.target, self.input.plan.presence)?;
        let conversion = callback.conversion::<S>(ident, &self.input.failure)?;

        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type.clone()],
            conversions: vec![conversion],
            writebacks: Vec::new(),
            argument: quote! { #ident },
        })
    }

    fn conversion(
        &self,
        class: &rust_api::ClassHandle,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let ident = &self.input.ident;
        let ty = class.ty();
        let handle_type = names::Class::from_local_type(ty)?.handle();
        let handle_pointer = quote! { #ident as usize as *mut #handle_type };
        let failure = &self.input.failure;
        let null_check = matches!(self.input.plan.presence, HandlePresence::Required).then(|| {
            quote! {
                if #ident == #zero {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null class handle",
                        stringify!(#ident)
                    ));
                    #failure
                }
            }
        });

        Ok(match (self.input.plan.receive, class.presence()) {
            (Receive::ByValue, HandlePresence::Required) => quote! {
                #null_check
                let #ident: #ty = match unsafe { #handle_type::take(#handle_pointer) } {
                    Some(value) => value,
                    None => {
                        ::boltffi::__private::set_last_error(format!(
                            "{}: released class handle",
                            stringify!(#ident)
                        ));
                        #failure
                    }
                };
            },
            (Receive::ByValue, HandlePresence::Nullable) => quote! {
                let #ident: Option<#ty> = if #ident == #zero {
                    None
                } else {
                    Some(match unsafe { #handle_type::take(#handle_pointer) } {
                        Some(value) => value,
                        None => {
                            ::boltffi::__private::set_last_error(format!(
                                "{}: released class handle",
                                stringify!(#ident)
                            ));
                            #failure
                        }
                    })
                };
            },
            (Receive::ByRef, HandlePresence::Required) => quote! {
                #null_check
                let #ident: &#ty = unsafe {
                    #handle_type::shared(#handle_pointer)
                };
            },
            (Receive::ByMutRef, HandlePresence::Required) => quote! {
                #null_check
                let #ident: &mut #ty = unsafe {
                    #handle_type::mutable(#handle_pointer)
                };
            },
            (Receive::ByRef | Receive::ByMutRef, HandlePresence::Nullable) => {
                return Err(Error::UnsupportedExpansion(
                    "nullable borrowed class handle",
                ));
            }
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown class handle receive mode",
                ));
            }
        })
    }
}

impl rust_api::CallbackObject {
    fn conversion<S>(&self, ident: &Ident, failure: &TokenStream) -> Result<TokenStream, Error>
    where
        S: crate::experimental::surface::RenderSurface,
        for<'ident> CallbackCarrier: Render<S, CallbackHandleInput<'ident>, Output = TokenStream>,
    {
        let handle = names::Parameter::new(ident).handle();
        let handle_binding = <CallbackCarrier as Render<S, _>>::render(
            CallbackCarrier,
            CallbackHandleInput::new(ident),
        )?;
        let value = self.value_from_handle(&quote! { #handle })?;
        let ty = self.value();
        match self.presence() {
            HandlePresence::Required => Ok(quote! {
                let #handle = #handle_binding;
                if #handle.is_null() {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null callback handle",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let #ident: #ty = unsafe {
                    #value
                };
            }),
            HandlePresence::Nullable => Ok(quote! {
                let #handle = #handle_binding;
                let #ident: Option<#ty> = if #handle.is_null() {
                    None
                } else {
                    Some(unsafe {
                        #value
                    })
                };
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown callback handle presence",
            )),
        }
    }

    fn value_from_handle(&self, handle: &TokenStream) -> Result<TokenStream, Error> {
        let object = self.object();
        Ok(match self.form() {
            rust_api::CallbackCarrier::BoxedDyn => {
                quote! {
                    <#object as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                }
            }
            rust_api::CallbackCarrier::ArcDyn => {
                quote! {
                    <#object as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(#handle)
                }
            }
        })
    }
}

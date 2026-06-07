use boltffi_binding::{HandlePresence, HandleTarget, Native, Receive, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Ident;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, callable::signature, local},
};

use super::Tokens;

pub struct Rule;
struct CallbackCarrier;

pub struct Plan<'binding, C> {
    target: &'binding HandleTarget,
    carrier: C,
    presence: HandlePresence,
    receive: Receive,
}

pub struct Input<'binding, C> {
    plan: Plan<'binding, C>,
    source: signature::Parameter<'binding>,
    ident: Ident,
    failure: TokenStream,
}

struct CallbackHandleInput<'a> {
    ident: &'a Ident,
}

impl<'a> CallbackHandleInput<'a> {
    fn new(ident: &'a Ident) -> Self {
        Self { ident }
    }
}

impl<'a> RenderRule<Native, CallbackHandleInput<'a>> for CallbackCarrier {
    type Output = TokenStream;

    fn apply(self, input: CallbackHandleInput<'a>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { #ident })
    }
}

impl<'a> RenderRule<Wasm32, CallbackHandleInput<'a>> for CallbackCarrier {
    type Output = TokenStream;

    fn apply(self, input: CallbackHandleInput<'a>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { ::boltffi::__private::CallbackHandle::from_wasm_handle(#ident) })
    }
}

impl<'binding, C> Plan<'binding, C> {
    pub fn new(
        target: &'binding HandleTarget,
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

impl<'binding, C> Input<'binding, C> {
    pub fn new(
        plan: Plan<'binding, C>,
        source: signature::Parameter<'binding>,
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

impl<'binding> RenderRule<Native, Input<'binding, native::HandleCarrier>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, native::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassParam::new(input).tokens::<Native>()
    }
}

impl<'binding> RenderRule<Wasm32, Input<'binding, wasm32::HandleCarrier>> for Rule {
    type Output = Tokens;

    fn apply(self, input: Input<'binding, wasm32::HandleCarrier>) -> Result<Self::Output, Error> {
        ClassParam::new(input).tokens::<Wasm32>()
    }
}

struct ClassParam<'binding, C> {
    input: Input<'binding, C>,
}

impl<'binding, C> ClassParam<'binding, C> {
    fn new(input: Input<'binding, C>) -> Self {
        Self { input }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: crate::experimental::target::Target<HandleCarrier = C>,
        for<'ident> CallbackCarrier:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
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
        S: crate::experimental::target::Target<HandleCarrier = C>,
        for<'ident> CallbackCarrier:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
    {
        let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
            render::handle::Carrier,
            render::handle::CarrierInput::new(self.input.plan.carrier),
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
        S: crate::experimental::target::Target<HandleCarrier = C>,
        for<'ident> CallbackCarrier:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
    {
        let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
            render::handle::Carrier,
            render::handle::CarrierInput::new(self.input.plan.carrier),
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
        class: &signature::ClassHandle,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let ident = &self.input.ident;
        let ty = class.ty();
        let mutable_pointer = quote! { #ident as usize as *mut #ty };
        let const_pointer = quote! { #ident as usize as *const #ty };
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
                let #ident: #ty = unsafe {
                    *Box::from_raw(#mutable_pointer)
                };
            },
            (Receive::ByValue, HandlePresence::Nullable) => quote! {
                let #ident: Option<#ty> = if #ident == #zero {
                    None
                } else {
                    Some(unsafe {
                        *Box::from_raw(#mutable_pointer)
                    })
                };
            },
            (Receive::ByRef, HandlePresence::Required) => quote! {
                #null_check
                let #ident: &#ty = unsafe {
                    &*(#const_pointer)
                };
            },
            (Receive::ByMutRef, HandlePresence::Required) => quote! {
                #null_check
                let #ident: &mut #ty = unsafe {
                    &mut *(#mutable_pointer)
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

impl signature::CallbackObject {
    fn conversion<S>(&self, ident: &Ident, failure: &TokenStream) -> Result<TokenStream, Error>
    where
        S: crate::experimental::target::Target,
        for<'ident> CallbackCarrier:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
    {
        let handle = local::Parameter::new(ident).handle();
        let handle_binding = <CallbackCarrier as RenderRule<S, _>>::apply(
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
            signature::CallbackCarrier::BoxedDyn => {
                quote! {
                    <#object as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                }
            }
            signature::CallbackCarrier::ArcDyn => {
                quote! {
                    <#object as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(#handle)
                }
            }
        })
    }
}

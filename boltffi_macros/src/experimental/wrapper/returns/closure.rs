use boltffi_binding::{
    ClosureForm, ClosureReturn, HandlePresence, Native, OutOfRust, Wasm32, native,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::Ident;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{self, Render, names},
};

use super::{RustInvocation, Tokens};

pub struct Renderer;
pub struct Write;

pub struct Input<'context, 'a, S: Target> {
    closure: &'a ClosureReturn<S, OutOfRust>,
    source: rust_api::Closure<'a>,
    invocation: RustInvocation,
    expansion: &'context Expansion<'a, S>,
}

pub struct WriteInput<'context, 'a, S: Target> {
    closure: &'a ClosureReturn<S, OutOfRust>,
    source: rust_api::Closure<'a>,
    value: Ident,
    owner: Ident,
    channel: ClosureReturnChannel,
    span: Span,
    expansion: &'context Expansion<'a, S>,
}

#[derive(Clone, Copy)]
enum ClosureReturnChannel {
    Return,
    Success,
}

pub struct WriteTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    body: TokenStream,
}

impl<'context, 'a, S: Target> Input<'context, 'a, S> {
    pub fn new(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure<'a>,
        invocation: RustInvocation,
        expansion: &'context Expansion<'a, S>,
    ) -> Self {
        Self {
            closure,
            source,
            invocation,
            expansion,
        }
    }
}

impl<'context, 'a, S: Target> WriteInput<'context, 'a, S> {
    pub fn returned(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure<'a>,
        value: Ident,
        owner: Ident,
        expansion: &'context Expansion<'a, S>,
    ) -> Self {
        Self::new(
            closure,
            source,
            value,
            owner,
            ClosureReturnChannel::Return,
            expansion,
        )
    }

    pub fn success(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure<'a>,
        value: Ident,
        owner: Ident,
        expansion: &'context Expansion<'a, S>,
    ) -> Self {
        Self::new(
            closure,
            source,
            value,
            owner,
            ClosureReturnChannel::Success,
            expansion,
        )
    }

    fn new(
        closure: &'a ClosureReturn<S, OutOfRust>,
        source: rust_api::Closure<'a>,
        value: Ident,
        owner: Ident,
        channel: ClosureReturnChannel,
        expansion: &'context Expansion<'a, S>,
    ) -> Self {
        let span = owner.span();
        Self {
            closure,
            source,
            value,
            owner,
            channel,
            span,
            expansion,
        }
    }
}

impl ClosureReturnChannel {
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

impl<'context, 'a, S> Render<S, Input<'context, 'a, S>> for Renderer
where
    S: Target,
    Write: Render<S, WriteInput<'context, 'a, S>, Output = WriteTokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'context, 'a, S>) -> Result<Self::Output, Error> {
        let RustInvocation {
            function,
            conversions,
            writebacks,
            arguments,
        } = input.invocation;
        let value = names::Wrapper::new(function.span()).closure();
        let writer = <Write as Render<S, _>>::render(
            Write,
            WriteInput::returned(
                input.closure,
                input.source,
                value.clone(),
                function.clone(),
                input.expansion,
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

impl<'context, 'a> Render<Native, WriteInput<'context, 'a, Native>> for Write {
    type Output = WriteTokens;

    fn render(self, input: WriteInput<'context, 'a, Native>) -> Result<Self::Output, Error> {
        match input.closure.registration().shape() {
            native::ClosureRegistration::InvokeContextRelease => NativeClosure::new(input).tokens(),
            _ => Err(Error::UnsupportedExpansion(
                "unknown native closure return registration",
            )),
        }
    }
}

impl<'context, 'a> Render<Wasm32, WriteInput<'context, 'a, Wasm32>> for Write {
    type Output = WriteTokens;

    fn render(self, input: WriteInput<'context, 'a, Wasm32>) -> Result<Self::Output, Error> {
        WasmClosure::new(input).tokens()
    }
}

struct NativeClosure<'context, 'a> {
    input: WriteInput<'context, 'a, Native>,
}

impl<'context, 'a> NativeClosure<'context, 'a> {
    fn new(input: WriteInput<'context, 'a, Native>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<WriteTokens, Error> {
        let returned_closure = ReturnedClosure::new(self.input.source, self.input.closure)?;
        let invoke = wrapper::closure::Invoke::<Native>::new(
            self.input.closure.invoke(),
            self.input.source.signature(),
            &returned_closure.signature,
            self.input.expansion,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let invoke_parameters = invoke.parameters(&failure)?;
        let parameter_items = invoke_parameters.items().to_vec();
        let return_ffi_parameters = return_tokens.ffi_parameters();
        let return_ffi_parameter_types = return_tokens.ffi_parameter_types();
        let storage = format_ident!("__BoltffiClosureReturn{}", self.input.value);
        let channel = self.input.channel.suffix();
        let call = format_ident!("__boltffi_{}_{}_call", self.input.owner, channel);
        let release = format_ident!("__boltffi_{}_{}_release", self.input.owner, channel);
        let locals = names::Wrapper::new(self.input.span);
        let output = locals.return_out();
        let context = locals.closure_context();
        let ffi_parameter_types = invoke_parameters
            .ffi_parameter_types()
            .iter()
            .cloned()
            .chain(return_ffi_parameter_types)
            .collect::<Vec<_>>();
        let ffi_parameters = invoke_parameters
            .ffi_parameters()
            .iter()
            .cloned()
            .chain(return_ffi_parameters)
            .collect::<Vec<_>>();
        let conversions = invoke_parameters.conversions();
        let arguments = invoke_parameters.arguments();
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

struct WasmClosure<'context, 'a> {
    input: WriteInput<'context, 'a, Wasm32>,
}

impl<'context, 'a> WasmClosure<'context, 'a> {
    fn new(input: WriteInput<'context, 'a, Wasm32>) -> Self {
        Self { input }
    }

    fn tokens(self) -> Result<WriteTokens, Error> {
        let returned_closure = ReturnedClosure::new(self.input.source, self.input.closure)?;
        let invoke = wrapper::closure::Invoke::<Wasm32>::new(
            self.input.closure.invoke(),
            self.input.source.signature(),
            &returned_closure.signature,
            self.input.expansion,
        )?;
        let return_tokens = invoke.return_tokens()?;
        let failure = return_tokens.failure();
        let invoke_parameters = invoke.parameters(&failure)?;
        let parameter_items = invoke_parameters.items().to_vec();
        let return_ffi_parameters = return_tokens.ffi_parameters();
        let registration = self.input.closure.registration().shape();
        let call = Ident::new(registration.call().name().as_str(), self.input.span);
        let release = Ident::new(registration.free().name().as_str(), self.input.span);
        let output = names::Wrapper::new(self.input.span).return_out();
        let ffi_parameters = invoke_parameters
            .ffi_parameters()
            .iter()
            .cloned()
            .chain(return_ffi_parameters)
            .collect::<Vec<_>>();
        let conversions = invoke_parameters.conversions();
        let arguments = invoke_parameters.arguments();
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

struct ReturnedClosure {
    kind: ReturnedClosureKind,
    form: ClosureForm,
    signature: wrapper::closure::Signature,
}

impl ReturnedClosure {
    fn new<S: Target>(
        source: rust_api::Closure<'_>,
        closure: &ClosureReturn<S, OutOfRust>,
    ) -> Result<Self, Error> {
        if source.function() != closure.form() {
            return Err(Error::SourceSyntaxMismatch(
                "source closure return form does not match binding closure",
            ));
        }

        let kind = match (closure.presence(), source.form()) {
            (HandlePresence::Required, rust_api::ClosureSourceForm::FunctionPointer) => {
                ReturnedClosureKind::FunctionPointer
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::BoxedDyn) => {
                ReturnedClosureKind::Boxed
            }
            (HandlePresence::Required, rust_api::ClosureSourceForm::ImplTrait) => {
                ReturnedClosureKind::ImplTrait
            }
            (HandlePresence::Nullable, rust_api::ClosureSourceForm::NullableBoxedDyn) => {
                ReturnedClosureKind::NullableBoxed
            }
            _ => {
                return Err(Error::SourceSyntaxMismatch(
                    "source closure return form does not match binding closure",
                ));
            }
        };
        let signature = wrapper::closure::Signature::from_source(source.signature())?;

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
        let parameters = self.signature.parameters();
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

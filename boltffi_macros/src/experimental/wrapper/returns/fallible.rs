use boltffi_binding::{ErrorDecl, OutOfRust, ReadPlan, ReturnDecl, ReturnPlan, TypeRef};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Ident;

use crate::experimental::{
    error::Error,
    rust_api,
    target::Target,
    wrapper::{self, Render, names},
};

use super::{RustInvocation, Tokens, closure, encoded, handle};

pub struct Renderer;
pub struct Success;

pub struct Input<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    error: &'a ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'a>,
    invocation: RustInvocation,
}

pub struct SuccessInput<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    source: rust_api::Fallible<'a>,
    owner: Ident,
    span: Span,
}

impl<'a, S: Target> SuccessInput<'a, S> {
    pub fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        source: rust_api::Fallible<'a>,
        owner: Ident,
    ) -> Self {
        let span = owner.span();
        Self {
            returns,
            source,
            owner,
            span,
        }
    }
}

impl<'a, S: Target> Input<'a, S> {
    pub fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        error: &'a ErrorDecl<S, OutOfRust>,
        source: rust_api::Return<'a>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            returns,
            error,
            source,
            invocation,
        }
    }
}

impl<'a, S> Render<S, Input<'a, S>> for Renderer
where
    S: Target,
    encoded::Renderer: Render<S, encoded::Input<'a, S>, Output = encoded::Tokens>
        + Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
    Success: Render<S, SuccessInput<'a, S>, Output = SuccessTokens>,
    handle::Value:
        Render<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'a, S>) -> Result<Self::Output, Error> {
        match input.error {
            ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } => EncodedError::new(
                input.returns,
                codec,
                *shape,
                input.source.fallible()?,
                input.invocation,
            )
            .tokens(),
            ErrorDecl::StatusViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("status error return"))
            }
            ErrorDecl::StatusViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("status error out-pointer"))
            }
            ErrorDecl::EncodedViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("encoded error out-pointer"))
            }
            ErrorDecl::None(_) => Err(Error::UnsupportedExpansion("missing error channel")),
            _ => Err(Error::UnsupportedExpansion("unknown error channel")),
        }
    }
}

struct EncodedError<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    error_codec: &'a ReadPlan,
    error_shape: S::BufferShape,
    source: rust_api::Fallible<'a>,
    invocation: RustInvocation,
}

impl<'a, S: Target> EncodedError<'a, S> {
    fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        error_codec: &'a ReadPlan,
        error_shape: S::BufferShape,
        source: rust_api::Fallible<'a>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            returns,
            error_codec,
            error_shape,
            source,
            invocation,
        }
    }

    fn tokens(self) -> Result<Tokens, Error>
    where
        encoded::Renderer: Render<S, encoded::Input<'a, S>, Output = encoded::Tokens>
            + Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
        Success: Render<S, SuccessInput<'a, S>, Output = SuccessTokens>,
        handle::Value:
            Render<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
    {
        let locals = names::Wrapper::new(self.invocation.function.span());
        let error_ident = locals.error();
        let error = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Input::new(self.error_codec, self.error_shape, error_ident.clone()),
        )?;
        let empty_error = <encoded::Renderer as Render<S, _>>::render(
            encoded::Renderer,
            encoded::Empty::new(self.error_shape),
        )?;
        let return_type = error.return_type().clone();
        let error_value = error.value();
        let empty_error_value = empty_error.value();
        let success = <Success as Render<S, _>>::render(
            Success,
            SuccessInput::new(self.returns, self.source, self.invocation.function.clone()),
        )?;
        let (success_items, success_ffi_parameters, success_pattern, success_body) =
            success.into_parts();
        let RustInvocation {
            function,
            conversions,
            writebacks,
            arguments,
        } = self.invocation;
        let result = names::Wrapper::new(function.span()).result();
        let result_value = if writebacks.is_empty() {
            quote! { #function(#(#arguments),*) }
        } else {
            quote! {
                {
                    let #result = #function(#(#arguments),*);
                    #(#writebacks)*
                    #result
                }
            }
        };

        Ok(Tokens {
            items: success_items,
            ffi_parameters: success_ffi_parameters,
            return_type,
            body: quote! {
                #(#conversions)*
                match #result_value {
                    Ok(#success_pattern) => {
                        #success_body
                        #empty_error_value
                    }
                    Err(#error_ident) => {
                        #error_value
                    }
                }
            },
        })
    }
}

impl<'a, S> Render<S, SuccessInput<'a, S>> for Success
where
    S: Target,
    encoded::Renderer: Render<S, encoded::Input<'a, S>, Output = encoded::Tokens>,
    closure::Write: Render<S, closure::WriteInput<'a, S>, Output = closure::WriteTokens>,
    handle::Value:
        Render<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
{
    type Output = SuccessTokens;

    fn render(self, input: SuccessInput<'a, S>) -> Result<Self::Output, Error> {
        let locals = names::Wrapper::new(input.span);
        let success_ident = locals.success();
        match input.returns.plan() {
            ReturnPlan::Void => Ok(SuccessTokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                pattern: quote! { () },
                body: TokenStream::new(),
            }),
            ReturnPlan::DirectViaOutPointer {
                ty: TypeRef::Primitive(primitive),
            } => {
                let out = locals.return_out();
                let ty = TypeRef::Primitive(*primitive);
                let ty = <wrapper::type_ref::Renderer as Render<S, &TypeRef>>::render(
                    wrapper::type_ref::Renderer,
                    &ty,
                )?;
                Ok(SuccessTokens {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! { #out: *mut #ty }],
                    pattern: quote! { #success_ident },
                    body: quote! {
                        if !#out.is_null() {
                            unsafe {
                                *#out = #success_ident;
                            }
                        }
                    },
                })
            }
            ReturnPlan::DirectViaOutPointer { .. } => {
                let out = locals.return_out();
                let ok = input.source.ok_written_type()?;
                Ok(SuccessTokens {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! {
                        #out: *mut <#ok as ::boltffi::__private::Passable>::Out
                    }],
                    pattern: quote! { #success_ident },
                    body: quote! {
                        if !#out.is_null() {
                            unsafe {
                                *#out = ::boltffi::__private::Passable::pack(#success_ident);
                            }
                        }
                    },
                })
            }
            ReturnPlan::EncodedViaOutPointer { codec, shape, .. } => {
                let out = locals.return_out();
                let encoded = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Input::new(codec, *shape, success_ident.clone()),
                )?;
                let out_ty = encoded.return_type_without_arrow();
                let encoded_value = encoded.value();
                Ok(SuccessTokens {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! { #out: *mut #out_ty }],
                    pattern: quote! { #success_ident },
                    body: quote! {
                        if !#out.is_null() {
                            unsafe {
                                *#out = #encoded_value;
                            }
                        }
                    },
                })
            }
            ReturnPlan::HandleViaOutPointer {
                target,
                carrier,
                presence,
            } => {
                input.source.ok_handle(target, *presence)?;
                let out = locals.return_out();
                let handle = <handle::Value as Render<S, _>>::render(
                    handle::Value,
                    handle::ValueInput::new(target, *carrier, *presence, success_ident.clone()),
                )?;
                let out_ty = handle.ty();
                let handle_value = handle.value();
                Ok(SuccessTokens {
                    items: Vec::new(),
                    ffi_parameters: vec![quote! { #out: *mut #out_ty }],
                    pattern: quote! { #success_ident },
                    body: quote! {
                        if !#out.is_null() {
                            unsafe {
                                *#out = #handle_value;
                            }
                        }
                    },
                })
            }
            ReturnPlan::ClosureViaOutPointer(closure) => {
                let source_closure = input.source.ok_closure(closure.presence())?;
                let writer = <closure::Write as Render<S, _>>::render(
                    closure::Write,
                    closure::WriteInput::success(
                        closure,
                        source_closure,
                        success_ident.clone(),
                        input.owner,
                    ),
                )?;
                let (items, ffi_parameters, body) = writer.into_parts();
                Ok(SuccessTokens {
                    items,
                    ffi_parameters,
                    pattern: quote! { #success_ident },
                    body,
                })
            }
            _ => Err(Error::UnsupportedExpansion("fallible return shape")),
        }
    }
}

pub struct SuccessTokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    pattern: TokenStream,
    body: TokenStream,
}

impl SuccessTokens {
    pub fn into_parts(self) -> (Vec<TokenStream>, Vec<TokenStream>, TokenStream, TokenStream) {
        (self.items, self.ffi_parameters, self.pattern, self.body)
    }
}

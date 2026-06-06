use boltffi_binding::{ErrorDecl, OutOfRust, ReadPlan, ReturnDecl, ReturnPlan, TypeRef};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, callable::signature, local},
    target::Target,
};

use super::{RustInvocation, Tokens, closure, encoded, handle};

pub struct Rule;
pub struct Success;

pub struct Input<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    error: &'a ErrorDecl<S, OutOfRust>,
    source: signature::Return<'a>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
}

pub struct SuccessInput<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    source: signature::Fallible<'a>,
    rust_type: Option<Type>,
    owner: Ident,
    span: Span,
}

impl<'a, S: Target> SuccessInput<'a, S> {
    pub fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        source: signature::Fallible<'a>,
        rust_type: Option<Type>,
        owner: Ident,
    ) -> Self {
        let span = owner.span();
        Self {
            returns,
            source,
            rust_type,
            owner,
            span,
        }
    }
}

impl<'a, S: Target> Input<'a, S> {
    pub fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        error: &'a ErrorDecl<S, OutOfRust>,
        source: signature::Return<'a>,
        rust_type: Option<Type>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            returns,
            error,
            source,
            rust_type,
            invocation,
        }
    }
}

impl<'a, S> RenderRule<S, Input<'a, S>> for Rule
where
    S: Target,
    encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>
        + RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
    Success: RenderRule<S, SuccessInput<'a, S>, Output = SuccessTokens>,
    handle::Value:
        RenderRule<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'a, S>) -> Result<Self::Output, Error> {
        match input.error {
            ErrorDecl::EncodedViaReturnSlot { codec, shape, .. } => EncodedError::new(
                input.returns,
                codec,
                *shape,
                input.source.fallible()?,
                input.rust_type,
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
    source: signature::Fallible<'a>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
}

impl<'a, S: Target> EncodedError<'a, S> {
    fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        error_codec: &'a ReadPlan,
        error_shape: S::BufferShape,
        source: signature::Fallible<'a>,
        rust_type: Option<Type>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            returns,
            error_codec,
            error_shape,
            source,
            rust_type,
            invocation,
        }
    }

    fn tokens(self) -> Result<Tokens, Error>
    where
        encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>
            + RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
        Success: RenderRule<S, SuccessInput<'a, S>, Output = SuccessTokens>,
        handle::Value:
            RenderRule<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
    {
        let locals = local::Wrapper::new(self.invocation.function.span());
        let error_ident = locals.error();
        let error = <encoded::Rule as RenderRule<S, _>>::apply(
            encoded::Rule,
            encoded::Input::new(self.error_codec, self.error_shape, error_ident.clone()),
        )?;
        let empty_error = <encoded::Rule as RenderRule<S, _>>::apply(
            encoded::Rule,
            encoded::Empty::new(self.error_shape),
        )?;
        let return_type = error.return_type().clone();
        let error_value = error.value();
        let empty_error_value = empty_error.value();
        let success = <Success as RenderRule<S, _>>::apply(
            Success,
            SuccessInput::new(
                self.returns,
                self.source,
                self.rust_type,
                self.invocation.function.clone(),
            ),
        )?;
        let (success_items, success_ffi_parameters, success_pattern, success_body) =
            success.into_parts();
        let RustInvocation {
            function,
            conversions,
            writebacks,
            arguments,
        } = self.invocation;
        let result = local::Wrapper::new(function.span()).result();
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

impl<'a, S> RenderRule<S, SuccessInput<'a, S>> for Success
where
    S: Target,
    encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>,
    closure::Write: RenderRule<S, closure::WriteInput<'a, S>, Output = closure::WriteTokens>,
    handle::Value:
        RenderRule<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
{
    type Output = SuccessTokens;

    fn apply(self, input: SuccessInput<'a, S>) -> Result<Self::Output, Error> {
        let result_type = input.rust_type.as_ref().and_then(RustResult::parse).ok_or(
            Error::SourceSyntaxMismatch("fallible binding return requires a source Result type"),
        )?;
        let locals = local::Wrapper::new(input.span);
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
                let ty = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
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
                let ok = result_type.ok();
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
                let encoded = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
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
                let handle = <handle::Value as RenderRule<S, _>>::apply(
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
                let ok = result_type.ok().clone();
                let source_closure = input.source.ok_closure(closure.presence())?;
                let writer = <closure::Write as RenderRule<S, _>>::apply(
                    closure::Write,
                    closure::WriteInput::success(
                        closure,
                        source_closure,
                        ok,
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

struct RustResult {
    ok: Type,
}

impl RustResult {
    fn parse(ty: &Type) -> Option<Self> {
        let Type::Path(path) = ty else {
            return None;
        };
        let segment = path.path.segments.last()?;
        (segment.ident == "Result").then_some(())?;
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        let ok = arguments.args.iter().find_map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty.clone()),
            _ => None,
        })?;
        Some(Self { ok })
    }

    fn ok(&self) -> &Type {
        &self.ok
    }
}

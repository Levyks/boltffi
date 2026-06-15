use boltffi_binding::{CodecNode, ErrorDecl, OutOfRust, ReturnDecl, ReturnPlan, TypeRef};
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Type;

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    rust_api,
    target::Target,
    wrapper::{self, Render, names},
};

pub struct Renderer;
pub struct Failure;

pub mod closure;
pub mod direct_vec;
pub mod encoded;
pub mod fallible;
pub mod handle;
pub mod scalar_option;

pub struct RustInvocation {
    owner: syn::Ident,
    span: Span,
    call: TokenStream,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
}

impl RustInvocation {
    pub fn new(
        owner: syn::Ident,
        call: TokenStream,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
    ) -> Self {
        let span = owner.span();
        Self {
            owner,
            span,
            call,
            conversions,
            writebacks,
        }
    }

    pub fn function(
        function: syn::Ident,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
        arguments: Vec<TokenStream>,
    ) -> Self {
        let call = quote! { #function(#(#arguments),*) };
        Self::new(function, call, conversions, writebacks)
    }
}

pub struct Input<'expansion, 'lowered, S: Target> {
    returns: &'lowered ReturnDecl<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    source: rust_api::Return<'lowered>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: Target> Input<'expansion, 'lowered, S> {
    pub fn new(
        returns: &'lowered ReturnDecl<S, OutOfRust>,
        error: &'lowered ErrorDecl<S, OutOfRust>,
        source: rust_api::Return<'lowered>,
        rust_type: Option<Type>,
        invocation: RustInvocation,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            returns,
            error,
            source,
            rust_type,
            invocation,
            expansion,
        }
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    return_type: TokenStream,
    body: TokenStream,
}

pub struct FailureInput<'expansion, 'lowered, S: Target> {
    returns: &'lowered ReturnDecl<S, OutOfRust>,
    error: &'lowered ErrorDecl<S, OutOfRust>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: Target> FailureInput<'expansion, 'lowered, S> {
    pub fn new(
        returns: &'lowered ReturnDecl<S, OutOfRust>,
        error: &'lowered ErrorDecl<S, OutOfRust>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            returns,
            error,
            expansion,
        }
    }
}

impl Tokens {
    pub fn items(&self) -> &[TokenStream] {
        &self.items
    }

    pub fn ffi_parameters(&self) -> &[TokenStream] {
        &self.ffi_parameters
    }

    pub fn return_type(&self) -> &TokenStream {
        &self.return_type
    }

    pub fn body(&self) -> &TokenStream {
        &self.body
    }
}

impl<'expansion, 'lowered, S> Render<S, Input<'expansion, 'lowered, S>> for Renderer
where
    S: Target,
    closure::Renderer: Render<S, closure::Input<'expansion, 'lowered, S>, Output = Tokens>,
    encoded::Renderer:
        Render<S, encoded::Input<'expansion, 'lowered, 'lowered, S>, Output = encoded::Tokens>,
    direct_vec::Renderer: Render<S, direct_vec::Input, Output = Tokens>,
    fallible::Renderer: Render<S, fallible::Input<'expansion, 'lowered, S>, Output = Tokens>,
    handle::Value: Render<
            S,
            handle::ValueInput<'expansion, 'lowered, S, S::HandleCarrier>,
            Output = handle::ValueTokens,
        >,
    scalar_option::Renderer: Render<S, scalar_option::Input, Output = Tokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'expansion, 'lowered, S>) -> Result<Self::Output, Error> {
        if !matches!(input.error, ErrorDecl::None(_)) {
            return <fallible::Renderer as Render<S, _>>::render(
                fallible::Renderer,
                fallible::Input::new(
                    input.returns,
                    input.error,
                    input.source,
                    input.invocation,
                    input.expansion,
                ),
            );
        }

        if let ReturnPlan::ClosureViaOutPointer(closure) = input.returns.plan() {
            return <closure::Renderer as Render<S, _>>::render(
                closure::Renderer,
                closure::Input::new(
                    closure,
                    input.source.closure(closure.presence())?,
                    input.invocation,
                    input.expansion,
                ),
            );
        }

        let RustInvocation {
            span,
            call,
            conversions,
            writebacks,
            ..
        } = input.invocation;
        let locals = names::Wrapper::new(span);
        match input.returns.plan() {
            ReturnPlan::Void => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> ::boltffi::__private::FfiStatus },
                body: quote! {
                    #(#conversions)*
                    #call;
                    #(#writebacks)*
                    ::boltffi::__private::FfiStatus::OK
                },
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let ty = TypeRef::Primitive(*primitive);
                let ty = <wrapper::type_ref::Renderer as Render<S, &TypeRef>>::render(
                    wrapper::type_ref::Renderer,
                    &ty,
                )?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        #call
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        #result
                    }
                };
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #ty },
                    body,
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding direct return requires a source return type",
                ))?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        ::boltffi::__private::Passable::pack(#call)
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        ::boltffi::__private::Passable::pack(#result)
                    }
                };
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out },
                    body,
                })
            }
            ReturnPlan::EncodedViaReturnSlot { codec, shape, .. } => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding encoded return requires a source return type",
                ))?;
                let result = locals.result();
                let encoded = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Input::new(codec, *shape, result.clone(), input.expansion),
                )?;
                let return_type = encoded.return_type().clone();
                let value = encoded.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::HandleViaReturnSlot {
                target,
                carrier,
                presence,
            } => {
                let handle_return = input.source.handle_return(target, *presence)?;
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding handle return requires a source return type",
                ))?;
                let result = locals.result();
                let handle = <handle::Value as Render<S, _>>::render(
                    handle::Value,
                    handle::ValueInput::new(
                        input.expansion,
                        target,
                        *carrier,
                        *presence,
                        result.clone(),
                        handle_return,
                    ),
                )?;
                let return_type = handle.ty();
                let value = handle.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type: quote! { -> #return_type },
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                input.source.scalar_option(*primitive)?;
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding scalar option return requires a source return type",
                ))?;
                let result = locals.result();
                let optional = <scalar_option::Renderer as Render<S, _>>::render(
                    scalar_option::Renderer,
                    scalar_option::Input::new(*primitive, result.clone()),
                )?;
                let return_type = optional.return_type;
                let body = optional.body;
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #call;
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                input.source.direct_vec()?;
                let result = locals.result();
                let sequence = <direct_vec::Renderer as Render<S, _>>::render(
                    direct_vec::Renderer,
                    direct_vec::Input::new(result.clone()),
                )?;
                let return_type = sequence.return_type;
                let body = sequence.body;
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result = #call;
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("direct out-pointer return"))
            }
            ReturnPlan::EncodedViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("encoded out-pointer return"))
            }
            ReturnPlan::HandleViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("handle out-pointer return"))
            }
            ReturnPlan::ClosureViaOutPointer(_) => {
                Err(Error::UnsupportedExpansion("closure out-pointer return"))
            }
            _ => Err(Error::UnsupportedExpansion("unknown return")),
        }
    }
}

impl<'expansion, 'lowered, S> Render<S, FailureInput<'expansion, 'lowered, S>> for Failure
where
    S: Target,
    direct_vec::Failure: Render<S, direct_vec::FailureInput, Output = TokenStream>,
    encoded::Renderer: Render<S, encoded::Empty<S>, Output = encoded::Tokens>,
    encoded::Renderer:
        Render<S, encoded::Input<'expansion, 'lowered, 'lowered, S>, Output = encoded::Tokens>,
    handle::Failure: Render<S, handle::FailureInput<S::HandleCarrier>, Output = TokenStream>,
    scalar_option::Failure: Render<S, scalar_option::FailureInput, Output = TokenStream>,
{
    type Output = TokenStream;

    fn render(self, input: FailureInput<'expansion, 'lowered, S>) -> Result<Self::Output, Error> {
        if !matches!(input.error, ErrorDecl::None(_)) {
            return ErrorFailure::new(input.error, input.expansion).tokens();
        }

        match input.returns.plan() {
            ReturnPlan::Void => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(_),
            } => Ok(quote! {
                return ::core::default::Default::default();
            }),
            ReturnPlan::DirectViaReturnSlot { .. } => Ok(quote! {
                return unsafe {
                    ::core::mem::MaybeUninit::zeroed().assume_init()
                };
            }),
            ReturnPlan::EncodedViaReturnSlot { shape, .. } => {
                let empty = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Empty::new(*shape),
                )?;
                let value = empty.value();
                Ok(quote! {
                    return #value;
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { .. } => {
                <scalar_option::Failure as Render<S, _>>::render(
                    scalar_option::Failure,
                    scalar_option::FailureInput,
                )
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                <direct_vec::Failure as Render<S, _>>::render(
                    direct_vec::Failure,
                    direct_vec::FailureInput,
                )
            }
            ReturnPlan::HandleViaReturnSlot {
                target, carrier, ..
            } => <handle::Failure as Render<S, _>>::render(
                handle::Failure,
                handle::FailureInput::new(target.clone(), *carrier),
            ),
            ReturnPlan::ClosureViaOutPointer(_) => Ok(quote! {
                return ::boltffi::__private::FfiStatus::INVALID_ARG;
            }),
            _ => Err(Error::UnsupportedExpansion("return failure")),
        }
    }
}

struct ErrorFailure<'expansion, 'lowered, S: Target> {
    error: &'lowered ErrorDecl<S, OutOfRust>,
    expansion: &'expansion Expansion<'lowered, S>,
}

impl<'expansion, 'lowered, S: Target> ErrorFailure<'expansion, 'lowered, S> {
    fn new(
        error: &'lowered ErrorDecl<S, OutOfRust>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { error, expansion }
    }

    fn tokens(self) -> Result<TokenStream, Error>
    where
        encoded::Renderer:
            Render<S, encoded::Input<'expansion, 'lowered, 'lowered, S>, Output = encoded::Tokens>,
    {
        match self.error {
            ErrorDecl::EncodedViaReturnSlot { codec, shape, .. }
                if matches!(codec.root(), CodecNode::String) =>
            {
                let error = names::Wrapper::new(proc_macro2::Span::call_site()).error();
                let encoded = <encoded::Renderer as Render<S, _>>::render(
                    encoded::Renderer,
                    encoded::Input::string(codec, *shape, error.clone(), self.expansion),
                )?;
                let value = encoded.value();
                Ok(quote! {
                    let #error = String::from("invalid argument");
                    return #value;
                })
            }
            ErrorDecl::EncodedViaReturnSlot { .. } => Err(Error::UnsupportedExpansion(
                "non-string encoded error failure",
            )),
            ErrorDecl::StatusViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("status error failure"))
            }
            _ => Err(Error::UnsupportedExpansion("error failure")),
        }
    }
}

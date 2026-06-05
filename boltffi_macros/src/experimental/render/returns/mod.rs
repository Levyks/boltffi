use boltffi_binding::{ErrorDecl, OutOfRust, ReturnDecl, ReturnPlan, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, local},
    target::Target,
};

pub struct Rule;
pub struct Failure;

pub mod closure;
pub mod direct_vec;
pub mod encoded;
pub mod fallible;
pub mod handle;
pub mod scalar_option;

/// The original Rust function invocation prepared for return rendering.
///
/// Return rendering owns this value because the return plan decides how the
/// result of the Rust function call leaves the exported wrapper. The same
/// invocation can be returned directly, packed through `Passable`, or bound to
/// a temporary before producing a buffer value.
///
/// # Example
///
/// For this Rust function:
///
/// ```rust
/// pub fn greet(name: String) -> String {
///     format!("hello {name}")
/// }
/// ```
///
/// after parameter rendering, the invocation payload is:
///
/// ```text
/// function:
///     greet
///
/// conversions:
///     let name: String = unsafe {
///         <String as ::boltffi::__private::Passable>::unpack(name)
///     };
///
/// arguments:
///     name
/// ```
///
/// An encoded return plan can render that payload as:
///
/// ```text
/// let __boltffi_result: String = greet(name);
/// ::boltffi::__private::FfiBuf::wire_encode(&__boltffi_result)
/// ```
pub struct RustInvocation {
    function: syn::Ident,
    conversions: Vec<TokenStream>,
    writebacks: Vec<TokenStream>,
    arguments: Vec<TokenStream>,
}

impl RustInvocation {
    /// Creates an invocation from the original function name and rendered parameter fragments.
    pub fn new(
        function: syn::Ident,
        conversions: Vec<TokenStream>,
        writebacks: Vec<TokenStream>,
        arguments: Vec<TokenStream>,
    ) -> Self {
        Self {
            function,
            conversions,
            writebacks,
            arguments,
        }
    }
}

pub struct Input<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    error: &'a ErrorDecl<S, OutOfRust>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
}

impl<'a, S: Target> Input<'a, S> {
    pub fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        error: &'a ErrorDecl<S, OutOfRust>,
        rust_type: Option<Type>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            returns,
            error,
            rust_type,
            invocation,
        }
    }
}

pub struct Tokens {
    items: Vec<TokenStream>,
    ffi_parameters: Vec<TokenStream>,
    return_type: TokenStream,
    body: TokenStream,
}

pub struct FailureInput<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    error: &'a ErrorDecl<S, OutOfRust>,
}

impl<'a, S: Target> FailureInput<'a, S> {
    pub fn new(returns: &'a ReturnDecl<S, OutOfRust>, error: &'a ErrorDecl<S, OutOfRust>) -> Self {
        Self { returns, error }
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

impl<'a, S> RenderRule<S, Input<'a, S>> for Rule
where
    S: Target,
    closure::Rule: RenderRule<S, closure::Input<'a, S>, Output = Tokens>,
    encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>,
    direct_vec::Rule: RenderRule<S, direct_vec::Input, Output = Tokens>,
    fallible::Rule: RenderRule<S, fallible::Input<'a, S>, Output = Tokens>,
    handle::Value:
        RenderRule<S, handle::ValueInput<'a, S::HandleCarrier>, Output = handle::ValueTokens>,
    scalar_option::Rule: RenderRule<S, scalar_option::Input, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'a, S>) -> Result<Self::Output, Error> {
        if !matches!(input.error, ErrorDecl::None(_)) {
            return <fallible::Rule as RenderRule<S, _>>::apply(
                fallible::Rule,
                fallible::Input::new(
                    input.returns,
                    input.error,
                    input.rust_type,
                    input.invocation,
                ),
            );
        }

        if let ReturnPlan::ClosureViaOutPointer(closure) = input.returns.plan() {
            return <closure::Rule as RenderRule<S, _>>::apply(
                closure::Rule,
                closure::Input::new(closure, input.rust_type, input.invocation),
            );
        }

        let RustInvocation {
            function,
            conversions,
            writebacks,
            arguments,
        } = input.invocation;
        let locals = local::Wrapper::new(function.span());
        match input.returns.plan() {
            ReturnPlan::Void => Ok(Tokens {
                items: Vec::new(),
                ffi_parameters: Vec::new(),
                return_type: quote! { -> ::boltffi::__private::FfiStatus },
                body: quote! {
                    #(#conversions)*
                    #function(#(#arguments),*);
                    #(#writebacks)*
                    ::boltffi::__private::FfiStatus::OK
                },
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let ty = TypeRef::Primitive(*primitive);
                let ty = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?;
                let body = if writebacks.is_empty() {
                    quote! {
                        #(#conversions)*
                        #function(#(#arguments),*)
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #function(#(#arguments),*);
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
                        ::boltffi::__private::Passable::pack(#function(#(#arguments),*))
                    }
                } else {
                    let result = locals.result();
                    quote! {
                        #(#conversions)*
                        let #result = #function(#(#arguments),*);
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
            ReturnPlan::EncodedViaReturnSlot { ty, shape, .. } => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding encoded return requires a source return type",
                ))?;
                let result = locals.result();
                let encoded = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
                    encoded::Input::new(ty, *shape, result.clone()),
                )?;
                let return_type = encoded.return_type().clone();
                let value = encoded.value();
                Ok(Tokens {
                    items: Vec::new(),
                    ffi_parameters: Vec::new(),
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #function(#(#arguments),*);
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
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding handle return requires a source return type",
                ))?;
                let result = locals.result();
                let handle = <handle::Value as RenderRule<S, _>>::apply(
                    handle::Value,
                    handle::ValueInput::new(
                        target,
                        *carrier,
                        *presence,
                        rust_type.clone(),
                        result.clone(),
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
                        let #result: #rust_type = #function(#(#arguments),*);
                        #(#writebacks)*
                        #value
                    },
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding scalar option return requires a source return type",
                ))?;
                let result = locals.result();
                let optional = <scalar_option::Rule as RenderRule<S, _>>::apply(
                    scalar_option::Rule,
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
                        let #result: #rust_type = #function(#(#arguments),*);
                        #(#writebacks)*
                        #body
                    },
                })
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                let result = locals.result();
                let sequence = <direct_vec::Rule as RenderRule<S, _>>::apply(
                    direct_vec::Rule,
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
                        let #result = #function(#(#arguments),*);
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

impl<'a, S> RenderRule<S, FailureInput<'a, S>> for Failure
where
    S: Target,
    direct_vec::Failure: RenderRule<S, direct_vec::FailureInput, Output = TokenStream>,
    encoded::Rule: RenderRule<S, encoded::Empty<S>, Output = encoded::Tokens>,
    encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>,
    handle::Failure: RenderRule<S, handle::FailureInput<S::HandleCarrier>, Output = TokenStream>,
    scalar_option::Failure: RenderRule<S, scalar_option::FailureInput, Output = TokenStream>,
{
    type Output = TokenStream;

    fn apply(self, input: FailureInput<'a, S>) -> Result<Self::Output, Error> {
        if !matches!(input.error, ErrorDecl::None(_)) {
            return ErrorFailure::new(input.error).tokens();
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
                let empty = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
                    encoded::Empty::new(*shape),
                )?;
                let value = empty.value();
                Ok(quote! {
                    return #value;
                })
            }
            ReturnPlan::ScalarOptionViaReturnSlot { .. } => {
                <scalar_option::Failure as RenderRule<S, _>>::apply(
                    scalar_option::Failure,
                    scalar_option::FailureInput,
                )
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                <direct_vec::Failure as RenderRule<S, _>>::apply(
                    direct_vec::Failure,
                    direct_vec::FailureInput,
                )
            }
            ReturnPlan::HandleViaReturnSlot {
                target, carrier, ..
            } => <handle::Failure as RenderRule<S, _>>::apply(
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

struct ErrorFailure<'a, S: Target> {
    error: &'a ErrorDecl<S, OutOfRust>,
}

impl<'a, S: Target> ErrorFailure<'a, S> {
    fn new(error: &'a ErrorDecl<S, OutOfRust>) -> Self {
        Self { error }
    }

    fn tokens(self) -> Result<TokenStream, Error>
    where
        encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>,
    {
        match self.error {
            ErrorDecl::EncodedViaReturnSlot { ty, shape, .. } if matches!(ty, TypeRef::String) => {
                let error = local::Wrapper::new(proc_macro2::Span::call_site()).error();
                let encoded = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
                    encoded::Input::new(ty, *shape, error.clone()),
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

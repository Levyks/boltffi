use boltffi_binding::{OutOfRust, ReturnDecl, ReturnPlan, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule;

mod encoded;

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
    arguments: Vec<TokenStream>,
}

impl RustInvocation {
    /// Creates an invocation from the original function name and rendered parameter fragments.
    pub fn new(
        function: syn::Ident,
        conversions: Vec<TokenStream>,
        arguments: Vec<TokenStream>,
    ) -> Self {
        Self {
            function,
            conversions,
            arguments,
        }
    }
}

pub struct Input<'a, S: Target> {
    returns: &'a ReturnDecl<S, OutOfRust>,
    rust_type: Option<Type>,
    invocation: RustInvocation,
}

impl<'a, S: Target> Input<'a, S> {
    pub fn new(
        returns: &'a ReturnDecl<S, OutOfRust>,
        rust_type: Option<Type>,
        invocation: RustInvocation,
    ) -> Self {
        Self {
            returns,
            rust_type,
            invocation,
        }
    }
}

pub struct Tokens {
    return_type: TokenStream,
    body: TokenStream,
}

impl Tokens {
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
    encoded::Rule: RenderRule<S, encoded::Input<'a, S>, Output = encoded::Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'a, S>) -> Result<Self::Output, Error> {
        let RustInvocation {
            function,
            conversions,
            arguments,
        } = input.invocation;
        match input.returns.plan() {
            ReturnPlan::Void => Ok(Tokens {
                return_type: quote! { -> ::boltffi::__private::FfiStatus },
                body: quote! {
                    #(#conversions)*
                    #function(#(#arguments),*);
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
                Ok(Tokens {
                    return_type: quote! { -> #ty },
                    body: quote! {
                        #(#conversions)*
                        #function(#(#arguments),*)
                    },
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding direct return requires a source return type",
                ))?;
                Ok(Tokens {
                    return_type: quote! { -> <#rust_type as ::boltffi::__private::Passable>::Out },
                    body: quote! {
                        #(#conversions)*
                        ::boltffi::__private::Passable::pack(#function(#(#arguments),*))
                    },
                })
            }
            ReturnPlan::EncodedViaReturnSlot { ty, shape, .. } => {
                let rust_type = input.rust_type.as_ref().ok_or(Error::SourceSyntaxMismatch(
                    "binding encoded return requires a source return type",
                ))?;
                let result = syn::Ident::new("__boltffi_result", function.span());
                let encoded = <encoded::Rule as RenderRule<S, _>>::apply(
                    encoded::Rule,
                    encoded::Input::new(ty, *shape, result.clone()),
                )?;
                let return_type = encoded.return_type().clone();
                let value = encoded.value();
                Ok(Tokens {
                    return_type,
                    body: quote! {
                        #(#conversions)*
                        let #result: #rust_type = #function(#(#arguments),*);
                        #value
                    },
                })
            }
            ReturnPlan::HandleViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("handle return"))
            }
            ReturnPlan::ScalarOptionViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("scalar option return"))
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("direct vec return"))
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

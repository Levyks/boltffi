use boltffi_binding::{HandlePresence, HandleTarget, Native, Receive, Wasm32, native, wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{GenericArgument, PatType, PathArguments, Type, TypeParamBound};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, callable::signature, local},
};

use super::Tokens;

pub struct Rule;
struct CallbackHandle;

pub struct Plan<'binding, C> {
    target: &'binding HandleTarget,
    carrier: C,
    presence: HandlePresence,
    receive: Receive,
}

pub struct Input<'binding, 'syntax, C> {
    plan: Plan<'binding, C>,
    source: signature::Parameter<'binding>,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: TokenStream,
}

struct CallbackHandleInput<'a> {
    ident: &'a syn::Ident,
}

impl<'a> CallbackHandleInput<'a> {
    fn new(ident: &'a syn::Ident) -> Self {
        Self { ident }
    }
}

impl<'a> RenderRule<Native, CallbackHandleInput<'a>> for CallbackHandle {
    type Output = TokenStream;

    fn apply(self, input: CallbackHandleInput<'a>) -> Result<Self::Output, Error> {
        let ident = input.ident;
        Ok(quote! { #ident })
    }
}

impl<'a> RenderRule<Wasm32, CallbackHandleInput<'a>> for CallbackHandle {
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

impl<'binding, 'syntax, C> Input<'binding, 'syntax, C> {
    pub fn new(
        plan: Plan<'binding, C>,
        source: signature::Parameter<'binding>,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            plan,
            source,
            syntax,
            ident,
            failure,
        }
    }
}

impl<'binding, 'syntax> RenderRule<Native, Input<'binding, 'syntax, native::HandleCarrier>>
    for Rule
{
    type Output = Tokens;

    fn apply(
        self,
        input: Input<'binding, 'syntax, native::HandleCarrier>,
    ) -> Result<Self::Output, Error> {
        ClassParam::new(input).tokens::<Native>()
    }
}

impl<'binding, 'syntax> RenderRule<Wasm32, Input<'binding, 'syntax, wasm32::HandleCarrier>>
    for Rule
{
    type Output = Tokens;

    fn apply(
        self,
        input: Input<'binding, 'syntax, wasm32::HandleCarrier>,
    ) -> Result<Self::Output, Error> {
        ClassParam::new(input).tokens::<Wasm32>()
    }
}

struct ClassParam<'binding, 'syntax, C> {
    input: Input<'binding, 'syntax, C>,
}

impl<'binding, 'syntax, C> ClassParam<'binding, 'syntax, C> {
    fn new(input: Input<'binding, 'syntax, C>) -> Self {
        Self { input }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        C: Copy,
        S: crate::experimental::target::Target<HandleCarrier = C>,
        for<'ident> CallbackHandle:
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
        for<'ident> CallbackHandle:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
    {
        self.input
            .source
            .handle(self.input.plan.target, self.input.plan.presence)?;
        let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
            render::handle::Carrier,
            render::handle::CarrierInput::new(self.input.plan.carrier),
        )?;
        let ident = self.input.ident;
        let ffi_type = carrier.ty();
        let rust_type = ClassSyntax::new(
            self.input.syntax.ty.as_ref(),
            self.input.plan.receive,
            self.input.plan.presence,
        )?;
        let conversion = self.conversion(&rust_type, carrier.zero())?;

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
        for<'ident> CallbackHandle:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
        render::handle::Carrier:
            RenderRule<S, render::handle::CarrierInput<C>, Output = render::handle::CarrierTokens>,
    {
        self.input
            .source
            .handle(self.input.plan.target, self.input.plan.presence)?;
        let carrier = <render::handle::Carrier as RenderRule<S, _>>::apply(
            render::handle::Carrier,
            render::handle::CarrierInput::new(self.input.plan.carrier),
        )?;
        let ident = self.input.ident;
        let ffi_type = carrier.ty();
        let callback =
            CallbackSyntax::new(self.input.syntax.ty.as_ref(), self.input.plan.presence)?;
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
        rust_type: &ClassSyntax<'_>,
        zero: &TokenStream,
    ) -> Result<TokenStream, Error> {
        let ident = self.input.ident;
        let mutable_pointer = rust_type.mutable_pointer(ident);
        let const_pointer = rust_type.const_pointer(ident);
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

        Ok(match (self.input.plan.receive, rust_type) {
            (Receive::ByValue, ClassSyntax::Required { ty }) => quote! {
                #null_check
                let #ident: #ty = unsafe {
                    *Box::from_raw(#mutable_pointer)
                };
            },
            (Receive::ByValue, ClassSyntax::Nullable { ty }) => quote! {
                let #ident: Option<#ty> = if #ident == #zero {
                    None
                } else {
                    Some(unsafe {
                        *Box::from_raw(#mutable_pointer)
                    })
                };
            },
            (Receive::ByRef, ClassSyntax::Required { ty }) => quote! {
                #null_check
                let #ident: &#ty = unsafe {
                    &*(#const_pointer)
                };
            },
            (Receive::ByMutRef, ClassSyntax::Required { ty }) => quote! {
                #null_check
                let #ident: &mut #ty = unsafe {
                    &mut *(#mutable_pointer)
                };
            },
            (Receive::ByRef | Receive::ByMutRef, ClassSyntax::Nullable { .. }) => {
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

enum CallbackSyntax<'a> {
    Required(CallbackTraitObject<'a>),
    Nullable(CallbackTraitObject<'a>),
}

impl<'a> CallbackSyntax<'a> {
    fn new(ty: &'a Type, presence: HandlePresence) -> Result<Self, Error> {
        match presence {
            HandlePresence::Required => Ok(Self::Required(CallbackTraitObject::parse(ty).ok_or(
                Error::SourceSyntaxMismatch(
                    "required callback parameter syntax does not match binding handle",
                ),
            )?)),
            HandlePresence::Nullable => Self::nullable(ty),
            _ => Err(Error::UnsupportedExpansion(
                "unknown callback handle presence",
            )),
        }
    }

    fn nullable(ty: &'a Type) -> Result<Self, Error> {
        let Type::Path(path) = ty else {
            return Err(Error::SourceSyntaxMismatch(
                "nullable callback parameter syntax does not match binding presence",
            ));
        };
        let segment = path
            .path
            .segments
            .last()
            .ok_or(Error::SourceSyntaxMismatch(
                "nullable callback parameter syntax does not match binding presence",
            ))?;
        if segment.ident != "Option" {
            return Err(Error::SourceSyntaxMismatch(
                "nullable callback parameter syntax does not match binding presence",
            ));
        }
        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return Err(Error::SourceSyntaxMismatch(
                "nullable callback parameter syntax does not match binding presence",
            ));
        };
        let inner = arguments
            .args
            .iter()
            .find_map(|argument| match argument {
                GenericArgument::Type(ty) => Some(ty),
                _ => None,
            })
            .ok_or(Error::SourceSyntaxMismatch(
                "nullable callback parameter syntax does not match binding presence",
            ))?;
        Ok(Self::Nullable(CallbackTraitObject::parse(inner).ok_or(
            Error::SourceSyntaxMismatch(
                "nullable callback parameter syntax does not match binding handle",
            ),
        )?))
    }

    fn conversion<S>(&self, ident: &syn::Ident, failure: &TokenStream) -> Result<TokenStream, Error>
    where
        S: crate::experimental::target::Target,
        for<'ident> CallbackHandle:
            RenderRule<S, CallbackHandleInput<'ident>, Output = TokenStream>,
    {
        let handle = local::Parameter::new(ident).handle();
        let handle_binding = <CallbackHandle as RenderRule<S, _>>::apply(
            CallbackHandle,
            CallbackHandleInput::new(ident),
        )?;
        match self {
            Self::Required(callback) => {
                let value = callback.value(&quote! { #handle })?;
                let ty = callback.rust_type();
                Ok(quote! {
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
                })
            }
            Self::Nullable(callback) => {
                let value = callback.value(&quote! { #handle })?;
                let ty = callback.rust_type();
                Ok(quote! {
                    let #handle = #handle_binding;
                    let #ident: Option<#ty> = if #handle.is_null() {
                        None
                    } else {
                        Some(unsafe {
                            #value
                        })
                    };
                })
            }
        }
    }
}

struct CallbackTraitObject<'a> {
    ty: &'a Type,
    trait_path: &'a syn::Path,
    ownership: CallbackOwnership,
}

impl<'a> CallbackTraitObject<'a> {
    fn parse(ty: &'a Type) -> Option<Self> {
        Self::parse_container(ty, "Box", CallbackOwnership::Boxed)
            .or_else(|| Self::parse_container(ty, "Arc", CallbackOwnership::Shared))
    }

    fn parse_container(
        ty: &'a Type,
        container: &str,
        ownership: CallbackOwnership,
    ) -> Option<Self> {
        let Type::Path(path) = ty else {
            return None;
        };
        let segment = path.path.segments.last()?;
        if segment.ident != container {
            return None;
        }
        let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return None;
        };
        let inner = arguments.args.iter().find_map(|argument| match argument {
            GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })?;
        let Type::TraitObject(trait_object) = inner else {
            return None;
        };
        let trait_path = trait_object.bounds.iter().find_map(|bound| match bound {
            TypeParamBound::Trait(bound) => Some(&bound.path),
            _ => None,
        })?;
        Some(Self {
            ty,
            trait_path,
            ownership,
        })
    }

    fn rust_type(&self) -> &Type {
        self.ty
    }

    fn value(&self, handle: &TokenStream) -> Result<TokenStream, Error> {
        let trait_path = self.trait_path;
        Ok(match self.ownership {
            CallbackOwnership::Boxed => {
                quote! {
                    <dyn #trait_path as ::boltffi::__private::BoxFromCallbackHandle>::box_from_callback_handle(#handle)
                }
            }
            CallbackOwnership::Shared => {
                quote! {
                    <dyn #trait_path as ::boltffi::__private::ArcFromCallbackHandle>::arc_from_callback_handle(#handle)
                }
            }
        })
    }
}

#[derive(Clone, Copy)]
enum CallbackOwnership {
    Boxed,
    Shared,
}

enum ClassSyntax<'a> {
    Required { ty: &'a Type },
    Nullable { ty: &'a Type },
}

impl<'a> ClassSyntax<'a> {
    fn new(ty: &'a Type, receive: Receive, presence: HandlePresence) -> Result<Self, Error> {
        match (receive, presence) {
            (Receive::ByValue, HandlePresence::Nullable) => Self::nullable(ty),
            (_, HandlePresence::Required) => Ok(Self::Required {
                ty: Self::required(ty, receive)?,
            }),
            (_, HandlePresence::Nullable) => Self::nullable(ty),
            _ => Err(Error::UnsupportedExpansion("unknown class handle presence")),
        }
    }

    fn required(ty: &'a Type, receive: Receive) -> Result<&'a Type, Error> {
        match (receive, ty) {
            (Receive::ByRef, Type::Reference(reference)) if reference.mutability.is_none() => {
                Ok(reference.elem.as_ref())
            }
            (Receive::ByMutRef, Type::Reference(reference)) if reference.mutability.is_some() => {
                Ok(reference.elem.as_ref())
            }
            (Receive::ByValue, _) => Ok(ty),
            (Receive::ByRef, _) => Err(Error::SourceSyntaxMismatch(
                "shared-reference class parameter syntax does not match binding receive mode",
            )),
            (Receive::ByMutRef, _) => Err(Error::SourceSyntaxMismatch(
                "mutable-reference class parameter syntax does not match binding receive mode",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown class handle receive mode",
            )),
        }
    }

    fn nullable(ty: &'a Type) -> Result<Self, Error> {
        let Type::Path(path) = ty else {
            return Err(Error::SourceSyntaxMismatch(
                "nullable class parameter syntax does not match binding presence",
            ));
        };
        let segment = path
            .path
            .segments
            .last()
            .ok_or(Error::SourceSyntaxMismatch(
                "nullable class parameter syntax does not match binding presence",
            ))?;
        if segment.ident != "Option" {
            return Err(Error::SourceSyntaxMismatch(
                "nullable class parameter syntax does not match binding presence",
            ));
        }
        let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
            return Err(Error::SourceSyntaxMismatch(
                "nullable class parameter syntax does not match binding presence",
            ));
        };
        match arguments.args.iter().find_map(|argument| match argument {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        }) {
            Some(ty) => Ok(Self::Nullable { ty }),
            None => Err(Error::SourceSyntaxMismatch(
                "nullable class parameter syntax does not match binding presence",
            )),
        }
    }

    fn mutable_pointer(&self, ident: &syn::Ident) -> TokenStream {
        match self {
            Self::Required { ty } | Self::Nullable { ty } => {
                quote! { #ident as usize as *mut #ty }
            }
        }
    }

    fn const_pointer(&self, ident: &syn::Ident) -> TokenStream {
        match self {
            Self::Required { ty } | Self::Nullable { ty } => {
                quote! { #ident as usize as *const #ty }
            }
        }
    }
}

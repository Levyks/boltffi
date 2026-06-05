use boltffi_binding::{Native, Primitive, Receive, TypeRef, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{PatType, Type};

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule, local},
    target::Target,
};

use super::Tokens;

pub struct Rule;
pub struct Record;

pub struct Input<'binding, 'syntax> {
    ty: &'binding TypeRef,
    receive: Receive,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: TokenStream,
}

impl<'binding, 'syntax> Input<'binding, 'syntax> {
    pub fn new(
        ty: &'binding TypeRef,
        receive: Receive,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            ty,
            receive,
            syntax,
            ident,
            failure,
        }
    }
}

pub struct RecordInput<'syntax> {
    receive: Receive,
    syntax: &'syntax PatType,
    ident: &'syntax syn::Ident,
    failure: TokenStream,
}

impl<'syntax> RecordInput<'syntax> {
    fn new(
        receive: Receive,
        syntax: &'syntax PatType,
        ident: &'syntax syn::Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            receive,
            syntax,
            ident,
            failure,
        }
    }
}

impl<'binding, 'syntax, S> RenderRule<S, Input<'binding, 'syntax>> for Rule
where
    S: Target,
    for<'ty> render::type_ref::Rule: RenderRule<S, &'ty TypeRef, Output = TokenStream>,
    Record: RenderRule<S, RecordInput<'syntax>, Output = Tokens>,
{
    type Output = Tokens;

    fn apply(self, input: Input<'binding, 'syntax>) -> Result<Self::Output, Error> {
        match input.ty {
            TypeRef::Primitive(primitive) => {
                PrimitiveParam::new(*primitive, input.receive, input.ident).tokens::<S>()
            }
            TypeRef::Record(_) => <Record as RenderRule<S, _>>::apply(
                Record,
                RecordInput::new(input.receive, input.syntax, input.ident, input.failure),
            ),
            _ => PassableParam::new(input.receive, input.ident, Self::rust_type(&input)?).tokens(),
        }
    }
}

impl<'syntax> RenderRule<Native, RecordInput<'syntax>> for Record {
    type Output = Tokens;

    fn apply(self, input: RecordInput<'syntax>) -> Result<Self::Output, Error> {
        PassableParam::new(input.receive, input.ident, Rule::record_type(&input)?).tokens()
    }
}

impl<'syntax> RenderRule<Wasm32, RecordInput<'syntax>> for Record {
    type Output = Tokens;

    fn apply(self, input: RecordInput<'syntax>) -> Result<Self::Output, Error> {
        WasmRecordParam::new(
            input.receive,
            input.ident,
            Rule::record_type(&input)?,
            input.failure,
        )
        .tokens()
    }
}

impl Rule {
    fn rust_type<'syntax>(input: &Input<'_, 'syntax>) -> Result<&'syntax Type, Error> {
        Self::syntax_type(input.syntax, input.receive)
    }

    fn record_type<'syntax>(input: &RecordInput<'syntax>) -> Result<&'syntax Type, Error> {
        Self::syntax_type(input.syntax, input.receive)
    }

    fn syntax_type(syntax: &PatType, receive: Receive) -> Result<&Type, Error> {
        match (receive, syntax.ty.as_ref()) {
            (Receive::ByValue, ty) => Ok(ty),
            (Receive::ByRef, Type::Reference(reference)) if reference.mutability.is_none() => {
                Ok(reference.elem.as_ref())
            }
            (Receive::ByRef, _) => Err(Error::SourceSyntaxMismatch(
                "shared-reference parameter syntax does not match binding receive mode",
            )),
            (Receive::ByMutRef, Type::Reference(reference)) if reference.mutability.is_some() => {
                Ok(reference.elem.as_ref())
            }
            (Receive::ByMutRef, _) => Err(Error::SourceSyntaxMismatch(
                "mutable-reference direct parameter syntax does not match binding receive mode",
            )),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct parameter receive mode",
            )),
        }
    }

    fn argument(receive: Receive, ident: &syn::Ident) -> Result<TokenStream, Error> {
        match receive {
            Receive::ByValue => Ok(quote! { #ident }),
            Receive::ByRef => Ok(quote! { &#ident }),
            Receive::ByMutRef => Ok(quote! { &mut #ident }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct parameter receive mode",
            )),
        }
    }
}

struct PrimitiveParam<'a> {
    primitive: Primitive,
    receive: Receive,
    ident: &'a syn::Ident,
}

impl<'a> PrimitiveParam<'a> {
    fn new(primitive: Primitive, receive: Receive, ident: &'a syn::Ident) -> Self {
        Self {
            primitive,
            receive,
            ident,
        }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        S: Target,
        for<'ty> render::type_ref::Rule: RenderRule<S, &'ty TypeRef, Output = TokenStream>,
    {
        let ty = TypeRef::Primitive(self.primitive);
        let ident = self.ident;
        let ffi_type = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
            render::type_ref::Rule,
            &ty,
        )?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            conversions: self.conversions(),
            writebacks: Vec::new(),
            argument: Rule::argument(self.receive, ident)?,
        })
    }

    fn conversions(&self) -> Vec<TokenStream> {
        let ident = self.ident;
        match self.receive {
            Receive::ByMutRef => vec![quote! { let mut #ident = #ident; }],
            _ => Vec::new(),
        }
    }
}

struct PassableParam<'a> {
    receive: Receive,
    ident: &'a syn::Ident,
    rust_type: &'a Type,
}

impl<'a> PassableParam<'a> {
    fn new(receive: Receive, ident: &'a syn::Ident, rust_type: &'a Type) -> Self {
        Self {
            receive,
            ident,
            rust_type,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = self.ident;
        let rust_type = self.rust_type;
        let ffi_type = quote! { <#rust_type as ::boltffi::__private::Passable>::In };
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            conversions: self.conversions(),
            writebacks: Vec::new(),
            argument: Rule::argument(self.receive, ident)?,
        })
    }

    fn conversions(&self) -> Vec<TokenStream> {
        let ident = self.ident;
        let rust_type = self.rust_type;
        match self.receive {
            Receive::ByMutRef => vec![quote! {
                let mut #ident: #rust_type = unsafe {
                    <#rust_type as ::boltffi::__private::Passable>::unpack(#ident)
                };
            }],
            _ => vec![quote! {
                let #ident: #rust_type = unsafe {
                    <#rust_type as ::boltffi::__private::Passable>::unpack(#ident)
                };
            }],
        }
    }
}

struct WasmRecordParam<'a> {
    receive: Receive,
    ident: &'a syn::Ident,
    rust_type: &'a Type,
    failure: TokenStream,
}

impl<'a> WasmRecordParam<'a> {
    fn new(
        receive: Receive,
        ident: &'a syn::Ident,
        rust_type: &'a Type,
        failure: TokenStream,
    ) -> Self {
        Self {
            receive,
            ident,
            rust_type,
            failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = self.ident;
        let ffi_type = self.ffi_type()?;
        let out = local::Parameter::new(ident).writeback();
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            conversions: vec![self.conversion(&out)?],
            writebacks: self.writebacks(&out)?,
            argument: Rule::argument(self.receive, ident)?,
        })
    }

    fn ffi_type(&self) -> Result<TokenStream, Error> {
        match self.receive {
            Receive::ByMutRef => Ok(quote! { *mut u8 }),
            Receive::ByValue | Receive::ByRef => Ok(quote! { *const u8 }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }

    fn conversion(&self, out: &syn::Ident) -> Result<TokenStream, Error> {
        let ident = self.ident;
        let rust_type = self.rust_type;
        let failure = &self.failure;
        match self.receive {
            Receive::ByMutRef => Ok(quote! {
                let #out = #ident;
                if #out.is_null() {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null direct record pointer",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let mut #ident: #rust_type = unsafe {
                    let __boltffi_value =
                        ::core::ptr::read_unaligned(#out as *const <#rust_type as ::boltffi::__private::Passable>::In);
                    <#rust_type as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                };
            }),
            Receive::ByValue | Receive::ByRef => Ok(quote! {
                if #ident.is_null() {
                    ::boltffi::__private::set_last_error(format!(
                        "{}: null direct record pointer",
                        stringify!(#ident)
                    ));
                    #failure
                }
                let #ident: #rust_type = unsafe {
                    let __boltffi_value =
                        ::core::ptr::read_unaligned(#ident as *const <#rust_type as ::boltffi::__private::Passable>::In);
                    <#rust_type as ::boltffi::__private::Passable>::unpack(__boltffi_value)
                };
            }),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }

    fn writebacks(&self, out: &syn::Ident) -> Result<Vec<TokenStream>, Error> {
        let ident = self.ident;
        let rust_type = self.rust_type;
        match self.receive {
            Receive::ByMutRef => Ok(vec![quote! {
                unsafe {
                    ::core::ptr::write_unaligned(
                        #out as *mut <#rust_type as ::boltffi::__private::Passable>::In,
                        ::boltffi::__private::Passable::pack(#ident)
                    );
                }
            }]),
            Receive::ByValue | Receive::ByRef => Ok(Vec::new()),
            _ => Err(Error::UnsupportedExpansion(
                "unknown direct record receive mode",
            )),
        }
    }
}

use boltffi_binding::{Native, Primitive, Receive, TypeRef, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type};

use crate::experimental::{
    error::Error,
    target::Target,
    wrapper::{self, Render, names},
};

use super::Tokens;

pub struct Renderer;
pub struct Record;

pub struct Input<'lowered> {
    ty: &'lowered TypeRef,
    receive: Receive,
    rust_type: Type,
    ident: Ident,
    failure: TokenStream,
}

impl<'lowered> Input<'lowered> {
    pub fn new(
        ty: &'lowered TypeRef,
        receive: Receive,
        rust_type: Type,
        ident: Ident,
        failure: TokenStream,
    ) -> Self {
        Self {
            ty,
            receive,
            rust_type,
            ident,
            failure,
        }
    }
}

pub struct RecordInput {
    receive: Receive,
    rust_type: Type,
    ident: Ident,
    failure: TokenStream,
}

impl RecordInput {
    pub fn new(receive: Receive, rust_type: Type, ident: Ident, failure: TokenStream) -> Self {
        Self {
            receive,
            rust_type,
            ident,
            failure,
        }
    }
}

impl<'lowered, S> Render<S, Input<'lowered>> for Renderer
where
    S: Target,
    for<'ty> wrapper::type_ref::Renderer: Render<S, &'ty TypeRef, Output = TokenStream>,
    Record: Render<S, RecordInput, Output = Tokens>,
{
    type Output = Tokens;

    fn render(self, input: Input<'lowered>) -> Result<Self::Output, Error> {
        match input.ty {
            TypeRef::Primitive(primitive) => {
                PrimitiveParam::new(*primitive, input.receive, input.ident).tokens::<S>()
            }
            TypeRef::Record(_) => <Record as Render<S, _>>::render(
                Record,
                RecordInput::new(input.receive, input.rust_type, input.ident, input.failure),
            ),
            _ => PassableParam::new(input.receive, input.ident, input.rust_type).tokens(),
        }
    }
}

impl Render<Native, RecordInput> for Record {
    type Output = Tokens;

    fn render(self, input: RecordInput) -> Result<Self::Output, Error> {
        PassableParam::new(input.receive, input.ident, input.rust_type).tokens()
    }
}

impl Render<Wasm32, RecordInput> for Record {
    type Output = Tokens;

    fn render(self, input: RecordInput) -> Result<Self::Output, Error> {
        WasmRecordParam::new(input.receive, input.ident, input.rust_type, input.failure).tokens()
    }
}

impl Renderer {
    fn argument(receive: Receive, ident: &Ident) -> Result<TokenStream, Error> {
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

struct PrimitiveParam {
    primitive: Primitive,
    receive: Receive,
    ident: Ident,
}

impl PrimitiveParam {
    fn new(primitive: Primitive, receive: Receive, ident: Ident) -> Self {
        Self {
            primitive,
            receive,
            ident,
        }
    }

    fn tokens<S>(self) -> Result<Tokens, Error>
    where
        S: Target,
        for<'ty> wrapper::type_ref::Renderer: Render<S, &'ty TypeRef, Output = TokenStream>,
    {
        let ty = TypeRef::Primitive(self.primitive);
        let ident = &self.ident;
        let ffi_type = <wrapper::type_ref::Renderer as Render<S, &TypeRef>>::render(
            wrapper::type_ref::Renderer,
            &ty,
        )?;
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            conversions: self.conversions(),
            writebacks: Vec::new(),
            argument: Renderer::argument(self.receive, ident)?,
        })
    }

    fn conversions(&self) -> Vec<TokenStream> {
        let ident = &self.ident;
        match self.receive {
            Receive::ByMutRef => vec![quote! { let mut #ident = #ident; }],
            _ => Vec::new(),
        }
    }
}

struct PassableParam {
    receive: Receive,
    ident: Ident,
    rust_type: Type,
}

impl PassableParam {
    fn new(receive: Receive, ident: Ident, rust_type: Type) -> Self {
        Self {
            receive,
            ident,
            rust_type,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
        let ffi_type = quote! { <#rust_type as ::boltffi::__private::Passable>::In };
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            conversions: self.conversions(),
            writebacks: Vec::new(),
            argument: Renderer::argument(self.receive, ident)?,
        })
    }

    fn conversions(&self) -> Vec<TokenStream> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
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

struct WasmRecordParam {
    receive: Receive,
    ident: Ident,
    rust_type: Type,
    failure: TokenStream,
}

impl WasmRecordParam {
    fn new(receive: Receive, ident: Ident, rust_type: Type, failure: TokenStream) -> Self {
        Self {
            receive,
            ident,
            rust_type,
            failure,
        }
    }

    fn tokens(self) -> Result<Tokens, Error> {
        let ident = &self.ident;
        let ffi_type = self.ffi_type()?;
        let out = names::Parameter::new(ident).writeback();
        Ok(Tokens {
            items: Vec::new(),
            ffi_parameters: vec![quote! { #ident: #ffi_type }],
            ffi_parameter_types: vec![ffi_type],
            conversions: vec![self.conversion(&out)?],
            writebacks: self.writebacks(&out)?,
            argument: Renderer::argument(self.receive, ident)?,
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

    fn conversion(&self, out: &Ident) -> Result<TokenStream, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
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

    fn writebacks(&self, out: &Ident) -> Result<Vec<TokenStream>, Error> {
        let ident = &self.ident;
        let rust_type = &self.rust_type;
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

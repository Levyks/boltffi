use boltffi_ast::{
    AdditionalBound, BaseTrait, ConstExpr, FnSig, GenericArgument, Literal, MapKind,
    ParameterPassing, Path, PathRoot, ReturnDef, TraitBounds, TypeExpr,
};
use boltffi_binding::Receive;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Type, parse_str, parse2};

use crate::experimental::error::Error;

pub struct TypeTokens {
    ty: Type,
}

impl TypeTokens {
    pub fn new(type_expr: &TypeExpr) -> Result<Self, Error> {
        Ok(Self {
            ty: parse2(tokens(type_expr)?).map_err(|_| {
                Error::SourceSyntaxMismatch("source type expression is not a Rust type")
            })?,
        })
    }

    pub fn parameter(passing: ParameterPassing, type_expr: &TypeExpr) -> Result<Self, Error> {
        let type_tokens = tokens(type_expr)?;
        let tokens = match passing {
            ParameterPassing::Value => type_tokens,
            ParameterPassing::Ref => quote! { &#type_tokens },
            ParameterPassing::RefMut => quote! { &mut #type_tokens },
        };
        Ok(Self {
            ty: parse2(tokens).map_err(|_| {
                Error::SourceSyntaxMismatch("source parameter type is not Rust syntax")
            })?,
        })
    }

    pub fn into_type(self) -> Type {
        self.ty
    }
}

pub struct DecodeTarget {
    parameter: Type,
    owned: Type,
    borrow: DecodeBorrow,
}

impl DecodeTarget {
    pub fn new(
        passing: ParameterPassing,
        receive: Receive,
        type_expr: &TypeExpr,
    ) -> Result<Self, Error> {
        match (passing, receive) {
            (ParameterPassing::Value, Receive::ByValue) => {
                let ty = TypeTokens::new(type_expr)?.into_type();
                Ok(Self {
                    parameter: ty.clone(),
                    owned: ty,
                    borrow: DecodeBorrow::Owned,
                })
            }
            (ParameterPassing::Ref, Receive::ByRef) => Ok(Self::borrowed(
                TypeTokens::parameter(passing, type_expr)?.into_type(),
                type_expr,
                false,
            )?),
            (ParameterPassing::RefMut, Receive::ByMutRef) => Ok(Self::borrowed(
                TypeTokens::parameter(passing, type_expr)?.into_type(),
                type_expr,
                true,
            )?),
            _ => Err(Error::SourceSyntaxMismatch(
                "source parameter passing does not match binding receive mode",
            )),
        }
    }

    pub fn received(receive: Receive, type_expr: &TypeExpr) -> Result<Self, Error> {
        let passing = match receive {
            Receive::ByValue => ParameterPassing::Value,
            Receive::ByRef => ParameterPassing::Ref,
            Receive::ByMutRef => ParameterPassing::RefMut,
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown encoded parameter receive mode",
                ));
            }
        };
        Self::new(passing, receive, type_expr)
    }

    pub fn by_value(type_expr: &TypeExpr) -> Result<Self, Error> {
        Self::new(ParameterPassing::Value, Receive::ByValue, type_expr)
    }

    pub fn parameter(&self) -> &Type {
        &self.parameter
    }

    pub fn owned(&self) -> &Type {
        &self.owned
    }

    pub const fn borrow(&self) -> DecodeBorrow {
        self.borrow
    }

    fn borrowed(parameter: Type, type_expr: &TypeExpr, mutable: bool) -> Result<Self, Error> {
        let (owned, borrow) = match type_expr {
            TypeExpr::Str => (
                TypeTokens::new(&TypeExpr::String)?.into_type(),
                DecodeBorrow::Str { mutable },
            ),
            TypeExpr::Slice(element) => (
                TypeTokens::new(&TypeExpr::Vec(element.clone()))?.into_type(),
                DecodeBorrow::Slice { mutable },
            ),
            _ => (
                TypeTokens::new(type_expr)?.into_type(),
                DecodeBorrow::Value { mutable },
            ),
        };
        Ok(Self {
            parameter,
            owned,
            borrow,
        })
    }
}

#[derive(Clone, Copy)]
pub enum DecodeBorrow {
    Owned,
    Value { mutable: bool },
    Slice { mutable: bool },
    Str { mutable: bool },
}

impl DecodeBorrow {
    pub const fn mutable(self) -> bool {
        match self {
            Self::Owned => false,
            Self::Value { mutable } | Self::Slice { mutable } | Self::Str { mutable } => mutable,
        }
    }
}

pub fn tokens(type_expr: &TypeExpr) -> Result<TokenStream, Error> {
    Ok(match type_expr {
        TypeExpr::Primitive(primitive) => {
            let primitive_type: Type = parse_str(primitive.rust_name())
                .map_err(|_| Error::SourceSyntaxMismatch("primitive type is not Rust syntax"))?;
            quote! { #primitive_type }
        }
        TypeExpr::Unit => quote! { () },
        TypeExpr::String => quote! { String },
        TypeExpr::Str => quote! { str },
        TypeExpr::Record { path, .. }
        | TypeExpr::Enum { path, .. }
        | TypeExpr::Class { path, .. }
        | TypeExpr::Custom { path, .. } => path_tokens(path)?,
        TypeExpr::Dyn(bound) => {
            let bound = trait_bound_tokens(bound)?;
            quote! { dyn #bound }
        }
        TypeExpr::ImplTrait(bound) => {
            let bound = trait_bound_tokens(bound)?;
            quote! { impl #bound }
        }
        TypeExpr::Boxed(inner) => {
            let inner = tokens(inner)?;
            quote! { Box<#inner> }
        }
        TypeExpr::Arc(inner) => {
            let inner = tokens(inner)?;
            quote! { ::std::sync::Arc<#inner> }
        }
        TypeExpr::FnPtr(signature) => fn_pointer_tokens(signature)?,
        TypeExpr::Vec(element) => {
            let element = tokens(element)?;
            quote! { Vec<#element> }
        }
        TypeExpr::Slice(element) => {
            let element = tokens(element)?;
            quote! { [#element] }
        }
        TypeExpr::Option(inner) => {
            let inner = tokens(inner)?;
            quote! { Option<#inner> }
        }
        TypeExpr::Result { ok, err } => {
            let ok = tokens(ok)?;
            let err = tokens(err)?;
            quote! { Result<#ok, #err> }
        }
        TypeExpr::Tuple(elements) => tuple_tokens(elements)?,
        TypeExpr::Map { kind, key, value } => {
            let key = tokens(key)?;
            let value = tokens(value)?;
            match kind {
                MapKind::Hash => quote! { ::std::collections::HashMap<#key, #value> },
                MapKind::BTree => quote! { ::std::collections::BTreeMap<#key, #value> },
            }
        }
        TypeExpr::SelfType => quote! { Self },
        TypeExpr::Parameter(parameter) => {
            let ident: Ident = parse_str(&parameter.name)
                .map_err(|_| Error::SourceSyntaxMismatch("type parameter is not an identifier"))?;
            quote! { #ident }
        }
    })
}

fn trait_bound_tokens(bounds: &TraitBounds) -> Result<TokenStream, Error> {
    let base = match &bounds.base {
        BaseTrait::Named { path, .. } => path_tokens(path)?,
        BaseTrait::Function(function_trait) => {
            let name = function_trait.kind.as_ref();
            let trait_ident: Ident = parse_str(name).map_err(|_| {
                Error::SourceSyntaxMismatch("closure trait name is not an identifier")
            })?;
            let parameters = function_trait
                .signature
                .parameters
                .iter()
                .map(tokens)
                .collect::<Result<Vec<_>, _>>()?;
            match &function_trait.signature.returns {
                ReturnDef::Void => quote! { #trait_ident(#(#parameters),*) },
                ReturnDef::Value(return_type) => {
                    let return_type = tokens(return_type)?;
                    quote! { #trait_ident(#(#parameters),*) -> #return_type }
                }
            }
        }
    };
    let additional = bounds
        .bounds
        .iter()
        .map(additional_bound_tokens)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(quote! { #base #(+ #additional)* })
}

fn additional_bound_tokens(bound: &AdditionalBound) -> Result<TokenStream, Error> {
    match bound {
        AdditionalBound::AutoTrait(path) => path_tokens(path),
        AdditionalBound::Lifetime(lifetime) => parse_str::<syn::Lifetime>(lifetime)
            .map(|lifetime| quote! { #lifetime })
            .map_err(|_| Error::SourceSyntaxMismatch("trait lifetime bound is not Rust syntax")),
    }
}

fn fn_pointer_tokens(signature: &FnSig) -> Result<TokenStream, Error> {
    let parameters = signature
        .parameters
        .iter()
        .map(tokens)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(match &signature.returns {
        ReturnDef::Void => quote! { fn(#(#parameters),*) },
        ReturnDef::Value(return_type) => {
            let return_type = tokens(return_type)?;
            quote! { fn(#(#parameters),*) -> #return_type }
        }
    })
}

fn tuple_tokens(elements: &[TypeExpr]) -> Result<TokenStream, Error> {
    let elements = elements.iter().map(tokens).collect::<Result<Vec<_>, _>>()?;
    Ok(match elements.as_slice() {
        [] => quote! { () },
        [element] => quote! { (#element,) },
        _ => quote! { (#(#elements),*) },
    })
}

fn path_tokens(path: &Path) -> Result<TokenStream, Error> {
    let path = path_string(path)?;
    parse_str::<Type>(&path)
        .map(|ty| quote!(#ty))
        .map_err(|_| Error::SourceSyntaxMismatch("source path is not a Rust type path"))
}

fn path_string(path: &Path) -> Result<String, Error> {
    let prefix = match path.root {
        PathRoot::Relative => String::new(),
        PathRoot::Crate => "crate::".to_owned(),
        PathRoot::Self_ => "self::".to_owned(),
        PathRoot::Super(count) => {
            std::iter::repeat_n("super", count.get())
                .collect::<Vec<_>>()
                .join("::")
                + "::"
        }
        PathRoot::Absolute => "::".to_owned(),
    };
    let segments = path
        .segments
        .iter()
        .map(|segment| {
            let arguments = segment
                .arguments
                .iter()
                .map(generic_argument_string)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(match arguments.is_empty() {
                true => segment.name.as_str().to_owned(),
                false => format!("{}<{}>", segment.name.as_str(), arguments.join(", ")),
            })
        })
        .collect::<Result<Vec<_>, Error>>()?
        .join("::");
    Ok(format!("{prefix}{segments}"))
}

fn generic_argument_string(argument: &GenericArgument) -> Result<String, Error> {
    match argument {
        GenericArgument::Type(type_expr) => type_string(type_expr),
        GenericArgument::Const(value) => const_expr_string(value),
        GenericArgument::AssociatedType { name, type_expr } => {
            Ok(format!("{} = {}", name.as_str(), type_string(type_expr)?))
        }
    }
}

fn type_string(type_expr: &TypeExpr) -> Result<String, Error> {
    TypeTokens::new(type_expr).map(|tokens| {
        let ty = tokens.into_type();
        quote!(#ty).to_string()
    })
}

fn const_expr_string(value: &ConstExpr) -> Result<String, Error> {
    Ok(match value {
        ConstExpr::Raw(source) => source.clone(),
        ConstExpr::Path(path) => path_string(path)?,
        ConstExpr::Literal(literal) => literal_string(literal),
        ConstExpr::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(const_expr_string)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
        ConstExpr::Tuple(values) => format!(
            "({})",
            values
                .iter()
                .map(const_expr_string)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ),
    })
}

fn literal_string(literal: &Literal) -> String {
    match literal {
        Literal::Bool(value) => value.to_string(),
        Literal::Integer(value) => value.source.clone(),
        Literal::Float(value) => value.source.clone(),
        Literal::String(value) => format!("{value:?}"),
        Literal::Bytes(value) => format!("{value:?}"),
    }
}

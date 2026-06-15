use boltffi_ast::{MapKind, TypeExpr};
use boltffi_binding::{
    CodecNode, CustomConverterPath, CustomConverterPathRoot, CustomTypeConverter,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprPath, Index, parse_str};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    surface::RenderSurface,
    wrapper::{self, Render},
};

pub struct Incoming<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct Outgoing<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct BorrowedOutgoing<'expansion, 'lowered, S: RenderSurface> {
    codec: &'lowered CodecNode,
    expansion: &'expansion Expansion<'lowered, S>,
}

pub struct IncomingConversion {
    tokens: TokenStream,
    fallible: bool,
    changed: bool,
}

struct ConverterRenderer<'lowered> {
    converter: &'lowered CustomTypeConverter,
}

struct PathRenderer<'lowered> {
    path: &'lowered CustomConverterPath,
}

impl IncomingConversion {
    pub fn tokens(&self) -> &TokenStream {
        &self.tokens
    }

    pub const fn fallible(&self) -> bool {
        self.fallible
    }

    pub const fn changed(&self) -> bool {
        self.changed
    }

    fn value_or_return_error(&self) -> TokenStream {
        let tokens = self.tokens();
        match self.fallible {
            true => quote! {
                match #tokens {
                    Ok(value) => value,
                    Err(error) => return Err(error),
                }
            },
            false => quote! { #tokens },
        }
    }
}

impl<'expansion, 'lowered, S: RenderSurface> Incoming<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn convert(&self, value: TokenStream) -> Result<IncomingConversion, Error> {
        self.convert_node(self.codec, value)
    }

    pub fn decoded_type(&self, source: &TypeExpr) -> Result<Option<TokenStream>, Error> {
        match contains_custom(self.codec, self.expansion)? {
            true => representation_type(self.codec, source, self.expansion).map(Some),
            false => Ok(None),
        }
    }

    fn convert_node(
        &self,
        codec: &CodecNode,
        value: TokenStream,
    ) -> Result<IncomingConversion, Error> {
        match codec {
            CodecNode::Custom(id) => {
                let custom = self.expansion.custom_type(*id)?;
                let converter =
                    ConverterRenderer::new(custom.converters().try_from_ffi()).render()?;
                Ok(IncomingConversion {
                    tokens: quote! { (#converter)(#value) },
                    fallible: true,
                    changed: true,
                })
            }
            CodecNode::Optional(inner) => self.convert_optional(inner, value),
            CodecNode::Sequence { element, .. } => self.convert_sequence(element, value),
            CodecNode::Result { ok, err } => self.convert_result(ok, err, value),
            CodecNode::EncodedRecord(_) => Ok(IncomingConversion {
                tokens: value,
                fallible: false,
                changed: false,
            }),
            CodecNode::Tuple(elements) => self.convert_tuple(elements, value),
            CodecNode::Map { key, value: item } => self.convert_map(key, item, value),
            _ => Ok(IncomingConversion {
                tokens: value,
                fallible: false,
                changed: false,
            }),
        }
    }

    fn convert_optional(
        &self,
        inner: &CodecNode,
        value: TokenStream,
    ) -> Result<IncomingConversion, Error> {
        let inner = self.convert_node(inner, quote! { value })?;
        let inner_tokens = inner.tokens();
        Ok(match inner.fallible {
            true => IncomingConversion {
                tokens: quote! {
                    match #value {
                        Some(value) => #inner_tokens.map(Some),
                        None => Ok(None),
                    }
                },
                fallible: true,
                changed: true,
            },
            false => IncomingConversion {
                tokens: quote! { #value.map(|value| #inner_tokens) },
                fallible: false,
                changed: inner.changed,
            },
        })
    }

    fn convert_sequence(
        &self,
        element: &CodecNode,
        value: TokenStream,
    ) -> Result<IncomingConversion, Error> {
        let element = self.convert_node(element, quote! { value })?;
        let element_tokens = element.tokens();
        Ok(match element.fallible {
            true => IncomingConversion {
                tokens: quote! {
                    #value
                        .into_iter()
                        .map(|value| #element_tokens)
                        .collect::<Result<Vec<_>, _>>()
                },
                fallible: true,
                changed: true,
            },
            false => IncomingConversion {
                tokens: quote! {
                    #value
                        .into_iter()
                        .map(|value| #element_tokens)
                        .collect::<Vec<_>>()
                },
                fallible: false,
                changed: element.changed,
            },
        })
    }

    fn convert_result(
        &self,
        ok: &CodecNode,
        err: &CodecNode,
        value: TokenStream,
    ) -> Result<IncomingConversion, Error> {
        let ok = self.convert_node(ok, quote! { value })?;
        let err = self.convert_node(err, quote! { error })?;
        let ok_tokens = ok.tokens();
        let err_tokens = err.tokens();
        let ok_arm = match ok.fallible {
            true => quote! { #ok_tokens.map(Ok) },
            false => quote! { Ok(Ok(#ok_tokens)) },
        };
        let err_arm = match err.fallible {
            true => quote! { #err_tokens.map(Err) },
            false => quote! { Ok(Err(#err_tokens)) },
        };
        Ok(IncomingConversion {
            tokens: quote! {
                match #value {
                    Ok(value) => #ok_arm,
                    Err(error) => #err_arm,
                }
            },
            fallible: ok.fallible || err.fallible,
            changed: ok.changed || err.changed,
        })
    }

    fn convert_tuple(
        &self,
        elements: &[CodecNode],
        value: TokenStream,
    ) -> Result<IncomingConversion, Error> {
        let elements = elements
            .iter()
            .enumerate()
            .map(|(index, element)| {
                let index = Index::from(index);
                self.convert_node(element, quote! { (#value).#index })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let fallible = elements.iter().any(IncomingConversion::fallible);
        let changed = elements.iter().any(IncomingConversion::changed);
        let values = elements
            .iter()
            .map(IncomingConversion::value_or_return_error)
            .collect::<Vec<_>>();
        Ok(match fallible {
            true => IncomingConversion {
                tokens: quote! {
                    (|| {
                        Ok((#(#values,)*))
                    })()
                },
                fallible: true,
                changed,
            },
            false => IncomingConversion {
                tokens: quote! { (#(#values,)*) },
                fallible: false,
                changed,
            },
        })
    }

    fn convert_map(
        &self,
        key: &CodecNode,
        item: &CodecNode,
        value: TokenStream,
    ) -> Result<IncomingConversion, Error> {
        let key = self.convert_node(key, quote! { key })?;
        let item = self.convert_node(item, quote! { value })?;
        let key_value = key.value_or_return_error();
        let item_value = item.value_or_return_error();
        let fallible = key.fallible() || item.fallible();
        let changed = key.changed() || item.changed();
        Ok(match fallible {
            true => IncomingConversion {
                tokens: quote! {
                    #value
                        .into_iter()
                        .map(|(key, value)| {
                            Ok((#key_value, #item_value))
                        })
                        .collect::<Result<_, _>>()
                },
                fallible: true,
                changed,
            },
            false => IncomingConversion {
                tokens: quote! {
                    #value
                        .into_iter()
                        .map(|(key, value)| (#key_value, #item_value))
                        .collect::<_>()
                },
                fallible: false,
                changed,
            },
        })
    }
}

impl<'expansion, 'lowered, S: RenderSurface> Outgoing<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn has_custom_conversion(&self) -> Result<bool, Error> {
        contains_custom(self.codec, self.expansion)
    }

    pub fn convert(&self, value: TokenStream) -> Result<TokenStream, Error> {
        self.convert_node(self.codec, value)
    }

    fn convert_node(&self, codec: &CodecNode, value: TokenStream) -> Result<TokenStream, Error> {
        match codec {
            CodecNode::Custom(id) => {
                let custom = self.expansion.custom_type(*id)?;
                let converter = ConverterRenderer::new(custom.converters().into_ffi()).render()?;
                Ok(quote! { (#converter)(&#value) })
            }
            CodecNode::Optional(inner) => {
                let inner = self.convert_node(inner, quote! { value })?;
                Ok(quote! { #value.map(|value| #inner) })
            }
            CodecNode::Sequence { element, .. } => {
                let element = self.convert_node(element, quote! { value })?;
                Ok(quote! {
                    #value
                        .into_iter()
                        .map(|value| #element)
                        .collect::<Vec<_>>()
                })
            }
            CodecNode::Result { ok, err } => {
                let ok = self.convert_node(ok, quote! { value })?;
                let err = self.convert_node(err, quote! { error })?;
                Ok(quote! {
                    match #value {
                        Ok(value) => Ok(#ok),
                        Err(error) => Err(#err),
                    }
                })
            }
            CodecNode::EncodedRecord(_) => Ok(value),
            CodecNode::Tuple(elements) => self.convert_tuple(elements, value),
            CodecNode::Map { key, value: item } => self.convert_map(key, item, value),
            _ => Ok(value),
        }
    }

    fn convert_tuple(
        &self,
        elements: &[CodecNode],
        value: TokenStream,
    ) -> Result<TokenStream, Error> {
        let values = elements
            .iter()
            .enumerate()
            .map(|(index, element)| {
                let index = Index::from(index);
                self.convert_node(element, quote! { (#value).#index })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote! { (#(#values,)*) })
    }

    fn convert_map(
        &self,
        key: &CodecNode,
        item: &CodecNode,
        value: TokenStream,
    ) -> Result<TokenStream, Error> {
        let key = self.convert_node(key, quote! { key })?;
        let item = self.convert_node(item, quote! { value })?;
        Ok(quote! {
            #value
                .into_iter()
                .map(|(key, value)| (#key, #item))
                .collect::<Vec<_>>()
        })
    }
}

impl<'expansion, 'lowered, S: RenderSurface> BorrowedOutgoing<'expansion, 'lowered, S> {
    pub const fn new(
        codec: &'lowered CodecNode,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { codec, expansion }
    }

    pub fn has_custom_conversion(&self) -> Result<bool, Error> {
        contains_custom(self.codec, self.expansion)
    }

    pub fn convert(&self, value: TokenStream) -> Result<TokenStream, Error> {
        self.convert_node(self.codec, value)
    }

    fn convert_node(&self, codec: &CodecNode, value: TokenStream) -> Result<TokenStream, Error> {
        match codec {
            CodecNode::Custom(id) => {
                let custom = self.expansion.custom_type(*id)?;
                let converter = ConverterRenderer::new(custom.converters().into_ffi()).render()?;
                Ok(quote! { (#converter)(#value) })
            }
            CodecNode::Optional(inner) => {
                let inner = self.convert_node(inner, quote! { value })?;
                Ok(quote! { #value.as_ref().map(|value| #inner) })
            }
            CodecNode::Sequence { element, .. } => {
                let element = self.convert_node(element, quote! { value })?;
                Ok(quote! {
                    #value
                        .iter()
                        .map(|value| #element)
                        .collect::<Vec<_>>()
                })
            }
            CodecNode::Result { ok, err } => {
                let ok = self.convert_node(ok, quote! { value })?;
                let err = self.convert_node(err, quote! { error })?;
                Ok(quote! {
                    match #value {
                        Ok(value) => Ok(#ok),
                        Err(error) => Err(#err),
                    }
                })
            }
            CodecNode::EncodedRecord(_) => Ok(value),
            CodecNode::Tuple(elements) => self.convert_tuple(elements, value),
            CodecNode::Map { key, value: item } => self.convert_map(key, item, value),
            _ => Ok(value),
        }
    }

    fn convert_tuple(
        &self,
        elements: &[CodecNode],
        value: TokenStream,
    ) -> Result<TokenStream, Error> {
        let values = elements
            .iter()
            .enumerate()
            .map(|(index, element)| {
                let index = Index::from(index);
                self.convert_node(element, quote! { &(#value).#index })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote! { (#(#values,)*) })
    }

    fn convert_map(
        &self,
        key: &CodecNode,
        item: &CodecNode,
        value: TokenStream,
    ) -> Result<TokenStream, Error> {
        let key = self.convert_node(key, quote! { key })?;
        let item = self.convert_node(item, quote! { value })?;
        Ok(quote! {
            #value
                .iter()
                .map(|(key, value)| (#key, #item))
                .collect::<Vec<_>>()
        })
    }
}

impl<'lowered> ConverterRenderer<'lowered> {
    const fn new(converter: &'lowered CustomTypeConverter) -> Self {
        Self { converter }
    }

    fn render(self) -> Result<TokenStream, Error> {
        match self.converter {
            CustomTypeConverter::Path(path) => PathRenderer::new(path).render(),
            CustomTypeConverter::Expression(expression) => parse_str::<Expr>(expression.source())
                .map(|expression| quote! { #expression })
                .map_err(|_| Error::SourceSyntaxMismatch("custom converter is not Rust syntax")),
            _ => Err(Error::UnsupportedExpansion("unknown custom converter")),
        }
    }
}

impl<'lowered> PathRenderer<'lowered> {
    const fn new(path: &'lowered CustomConverterPath) -> Self {
        Self { path }
    }

    fn render(self) -> Result<TokenStream, Error> {
        parse_str::<ExprPath>(&self.source()?)
            .map(|path| quote! { #path })
            .map_err(|_| Error::SourceSyntaxMismatch("custom converter path is not Rust syntax"))
    }

    fn source(&self) -> Result<String, Error> {
        Ok(self.prefix()? + &self.segments())
    }

    fn prefix(&self) -> Result<String, Error> {
        Ok(match self.path.root() {
            CustomConverterPathRoot::Relative => String::new(),
            CustomConverterPathRoot::Crate => "crate::".to_owned(),
            CustomConverterPathRoot::Self_ => "self::".to_owned(),
            CustomConverterPathRoot::Super(levels) => {
                std::iter::repeat_n("super", levels.get())
                    .collect::<Vec<_>>()
                    .join("::")
                    + "::"
            }
            CustomConverterPathRoot::Absolute => "::".to_owned(),
            _ => {
                return Err(Error::UnsupportedExpansion(
                    "unknown custom converter path root",
                ));
            }
        })
    }

    fn segments(&self) -> String {
        self.path
            .segments()
            .iter()
            .map(|segment| segment.as_str())
            .collect::<Vec<_>>()
            .join("::")
    }
}

fn representation_type<S: RenderSurface>(
    codec: &CodecNode,
    source: &TypeExpr,
    expansion: &Expansion<'_, S>,
) -> Result<TokenStream, Error> {
    match codec {
        CodecNode::Primitive(primitive) => {
            let ty = boltffi_binding::TypeRef::Primitive(*primitive);
            <wrapper::type_ref::Renderer as Render<S, &boltffi_binding::TypeRef>>::render(
                wrapper::type_ref::Renderer,
                &ty,
            )
        }
        CodecNode::String => Ok(quote! { String }),
        CodecNode::Bytes => Ok(quote! { Vec<u8> }),
        CodecNode::Custom(id) => {
            let custom = expansion.custom_type(*id)?;
            <wrapper::type_ref::Renderer as Render<S, &boltffi_binding::TypeRef>>::render(
                wrapper::type_ref::Renderer,
                custom.representation(),
            )
        }
        CodecNode::Optional(inner) => {
            let TypeExpr::Option(source) = source else {
                return Err(Error::SourceSyntaxMismatch(
                    "optional codec does not match source type",
                ));
            };
            let inner = representation_type(inner, source, expansion)?;
            Ok(quote! { Option<#inner> })
        }
        CodecNode::Sequence { element, .. } => {
            let source = match source {
                TypeExpr::Vec(element) | TypeExpr::Slice(element) => element.as_ref(),
                _ => {
                    return Err(Error::SourceSyntaxMismatch(
                        "sequence codec does not match source type",
                    ));
                }
            };
            let element = representation_type(element, source, expansion)?;
            Ok(quote! { Vec<#element> })
        }
        CodecNode::Result { ok, err } => {
            let TypeExpr::Result {
                ok: source_ok,
                err: source_err,
            } = source
            else {
                return Err(Error::SourceSyntaxMismatch(
                    "result codec does not match source type",
                ));
            };
            let ok = representation_type(ok, source_ok, expansion)?;
            let err = representation_type(err, source_err, expansion)?;
            Ok(quote! { Result<#ok, #err> })
        }
        CodecNode::Tuple(elements) => {
            let TypeExpr::Tuple(source_elements) = source else {
                return Err(Error::SourceSyntaxMismatch(
                    "tuple codec does not match source type",
                ));
            };
            if elements.len() != source_elements.len() {
                return Err(Error::SourceSyntaxMismatch(
                    "tuple codec arity does not match source type",
                ));
            }
            let elements = elements
                .iter()
                .zip(source_elements)
                .map(|(element, source)| representation_type(element, source, expansion))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(quote! { (#(#elements,)*) })
        }
        CodecNode::Map { key, value } => {
            if contains_custom(key, expansion)? {
                return Err(Error::UnsupportedExpansion("custom encoded map key"));
            }
            let TypeExpr::Map {
                kind,
                key: source_key,
                value: source_value,
            } = source
            else {
                return Err(Error::SourceSyntaxMismatch(
                    "map codec does not match source type",
                ));
            };
            let key = representation_type(key, source_key, expansion)?;
            let value = representation_type(value, source_value, expansion)?;
            match kind {
                MapKind::Hash => Ok(quote! { ::std::collections::HashMap<#key, #value> }),
                MapKind::BTree => Ok(quote! { ::std::collections::BTreeMap<#key, #value> }),
            }
        }
        CodecNode::EncodedRecord(_) => Err(Error::UnsupportedExpansion(
            "encoded record representation type",
        )),
        _ => Err(Error::UnsupportedExpansion("codec representation type")),
    }
}

fn contains_custom<S: RenderSurface>(
    codec: &CodecNode,
    expansion: &Expansion<'_, S>,
) -> Result<bool, Error> {
    match codec {
        CodecNode::Custom(_) => Ok(true),
        CodecNode::EncodedRecord(_) => Ok(false),
        CodecNode::Optional(inner) | CodecNode::Sequence { element: inner, .. } => {
            contains_custom(inner, expansion)
        }
        CodecNode::Result { ok, err } => {
            Ok(contains_custom(ok, expansion)? || contains_custom(err, expansion)?)
        }
        CodecNode::Tuple(elements) => contains_any(elements.iter(), expansion),
        CodecNode::Map { key, value } => {
            Ok(contains_custom(key, expansion)? || contains_custom(value, expansion)?)
        }
        _ => Ok(false),
    }
}

fn contains_any<'lowered, S: RenderSurface>(
    mut codecs: impl Iterator<Item = &'lowered CodecNode>,
    expansion: &Expansion<'_, S>,
) -> Result<bool, Error> {
    codecs.try_fold(false, |found, codec| {
        Ok(found || contains_custom(codec, expansion)?)
    })
}

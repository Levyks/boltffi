use boltffi_binding::{
    CodecNode, CustomConverterPath, CustomConverterPathRoot, CustomTypeConverter,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Expr, ExprPath, parse_str};

use crate::experimental::{
    error::Error,
    expansion::Expansion,
    target::Target,
    wrapper::{self, Render},
};

pub struct Incoming<'context, 'a, S: Target> {
    codec: &'a CodecNode,
    expansion: &'context Expansion<'a, S>,
}

pub struct Outgoing<'context, 'a, S: Target> {
    codec: &'a CodecNode,
    expansion: &'context Expansion<'a, S>,
}

pub struct IncomingConversion {
    tokens: TokenStream,
    fallible: bool,
    changed: bool,
}

struct ConverterRenderer<'a> {
    converter: &'a CustomTypeConverter,
}

struct PathRenderer<'a> {
    path: &'a CustomConverterPath,
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
}

impl<'context, 'a, S: Target> Incoming<'context, 'a, S> {
    pub const fn new(codec: &'a CodecNode, expansion: &'context Expansion<'a, S>) -> Self {
        Self { codec, expansion }
    }

    pub fn convert(&self, value: TokenStream) -> Result<IncomingConversion, Error> {
        self.convert_node(self.codec, value)
    }

    pub fn decoded_type(&self) -> Result<Option<TokenStream>, Error> {
        match contains_custom(self.codec, self.expansion)? {
            true => representation_type(self.codec, self.expansion).map(Some),
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
            CodecNode::EncodedRecord(id) => match record_contains_custom(*id, self.expansion)? {
                true => Err(Error::UnsupportedExpansion(
                    "custom conversion inside encoded record codec",
                )),
                false => Ok(IncomingConversion {
                    tokens: value,
                    fallible: false,
                    changed: false,
                }),
            },
            CodecNode::Tuple(elements) => {
                let custom = contains_any(elements.iter(), self.expansion)?;
                Err(match custom {
                    true => Error::UnsupportedExpansion("custom conversion inside tuple codec"),
                    false => Error::UnsupportedExpansion("tuple codec conversion"),
                })
            }
            CodecNode::Map { key, value } => Err(
                match contains_custom(key, self.expansion)?
                    || contains_custom(value, self.expansion)?
                {
                    true => Error::UnsupportedExpansion("custom conversion inside map codec"),
                    false => Error::UnsupportedExpansion("map codec conversion"),
                },
            ),
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
}

impl<'context, 'a, S: Target> Outgoing<'context, 'a, S> {
    pub const fn new(codec: &'a CodecNode, expansion: &'context Expansion<'a, S>) -> Self {
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
            CodecNode::EncodedRecord(id) => match record_contains_custom(*id, self.expansion)? {
                true => Err(Error::UnsupportedExpansion(
                    "custom conversion inside encoded record codec",
                )),
                false => Ok(value),
            },
            CodecNode::Tuple(elements) => {
                let custom = contains_any(elements.iter(), self.expansion)?;
                Err(match custom {
                    true => Error::UnsupportedExpansion("custom conversion inside tuple codec"),
                    false => Error::UnsupportedExpansion("tuple codec conversion"),
                })
            }
            CodecNode::Map { key, value } => Err(
                match contains_custom(key, self.expansion)?
                    || contains_custom(value, self.expansion)?
                {
                    true => Error::UnsupportedExpansion("custom conversion inside map codec"),
                    false => Error::UnsupportedExpansion("map codec conversion"),
                },
            ),
            _ => Ok(value),
        }
    }
}

impl<'a> ConverterRenderer<'a> {
    const fn new(converter: &'a CustomTypeConverter) -> Self {
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

impl<'a> PathRenderer<'a> {
    const fn new(path: &'a CustomConverterPath) -> Self {
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

fn representation_type<S: Target>(
    codec: &CodecNode,
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
            let inner = representation_type(inner, expansion)?;
            Ok(quote! { Option<#inner> })
        }
        CodecNode::Sequence { element, .. } => {
            let element = representation_type(element, expansion)?;
            Ok(quote! { Vec<#element> })
        }
        CodecNode::Result { ok, err } => {
            let ok = representation_type(ok, expansion)?;
            let err = representation_type(err, expansion)?;
            Ok(quote! { Result<#ok, #err> })
        }
        CodecNode::EncodedRecord(id) if record_contains_custom(*id, expansion)? => Err(
            Error::UnsupportedExpansion("custom conversion inside encoded record codec"),
        ),
        _ => Err(Error::UnsupportedExpansion("codec representation type")),
    }
}

fn contains_custom<S: Target>(
    codec: &CodecNode,
    expansion: &Expansion<'_, S>,
) -> Result<bool, Error> {
    match codec {
        CodecNode::Custom(_) => Ok(true),
        CodecNode::EncodedRecord(id) => record_contains_custom(*id, expansion),
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

fn record_contains_custom<S: Target>(
    id: boltffi_binding::RecordId,
    expansion: &Expansion<'_, S>,
) -> Result<bool, Error> {
    let record = expansion.encoded_record(id)?;
    contains_any(
        record
            .fields()
            .iter()
            .map(|field| field.codec().read().root()),
        expansion,
    )
}

fn contains_any<'a, S: Target>(
    mut codecs: impl Iterator<Item = &'a CodecNode>,
    expansion: &Expansion<'_, S>,
) -> Result<bool, Error> {
    codecs.try_fold(false, |found, codec| {
        Ok(found || contains_custom(codec, expansion)?)
    })
}

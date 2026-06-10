use boltffi_ast::{FieldDef, MethodDef, Path as SourcePath, RecordDef, TypeExpr, Visibility};
use boltffi_binding::{
    CanonicalName, CodecNode, DirectRecordDecl, EncodedFieldDecl, EncodedRecordDecl, ExecutionDecl,
    ExportedCallable, ExportedMethodDecl, FieldKey, InitializerDecl, NativeSymbol, Receive,
    RecordDecl, SurfaceLower, WritePlan,
};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Ident, Path, Type, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    target::{DirectRecordCrossing, Target},
    wrapper::{self, Render, encoded, export, names},
};

/// A record declaration renderer for one target surface.
///
/// The renderer emits the runtime trait implementations that make a scanned Rust
/// record usable by generated wrappers. The record shape comes from the lowered
/// `RecordDecl`, so the generated code cannot reclassify the source struct.
pub struct Renderer<'expansion, 'lowered, S: Target> {
    pair: DeclarationPair<'lowered, RecordDef, RecordDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Direct<'expansion, 'lowered, S: Target> {
    source: &'lowered RecordDef,
    binding: &'lowered DirectRecordDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct Encoded<'expansion, 'lowered, S: Target> {
    source: &'lowered RecordDef,
    binding: &'lowered EncodedRecordDecl<S>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EncodedField<'expansion, 'lowered, S: Target> {
    source: &'lowered FieldDef,
    binding: &'lowered EncodedFieldDecl,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct EncodedFieldTokens {
    wire_size: TokenStream,
    encode_to: TokenStream,
    decode_from: TokenStream,
    initializer: Ident,
}

struct RecordExports<'expansion, 'lowered, S: Target> {
    source: &'lowered RecordDef,
    record: Ident,
    initializers: &'lowered [InitializerDecl<S>],
    methods: &'lowered [ExportedMethodDecl<S, NativeSymbol>],
    receiver: ReceiverKind<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct RecordExport<'expansion, 'lowered, S: Target> {
    source: &'lowered RecordDef,
    record: Ident,
    source_method: &'lowered MethodDef,
    symbol: &'lowered NativeSymbol,
    callable: &'lowered ExportedCallable<S>,
    receiver: ReceiverKind<'lowered>,
    expansion: &'expansion Expansion<'lowered, S>,
}

#[derive(Clone, Copy)]
enum ReceiverKind<'lowered> {
    None,
    Direct,
    Encoded { codec: &'lowered WritePlan },
}

impl<'expansion, 'lowered, S: Target> Renderer<'expansion, 'lowered, S> {
    /// Creates a renderer for one paired record declaration.
    pub fn new(
        pair: DeclarationPair<'lowered, RecordDef, RecordDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self { pair, expansion }
    }

    /// Renders the runtime trait implementations for the record.
    pub fn render(self) -> Result<TokenStream, Error>
    where
        S: Target,
        wrapper::arguments::SyncRenderer: Render<
                S,
                wrapper::arguments::Input<'expansion, 'lowered, S>,
                Output = wrapper::arguments::Tokens,
            >,
        wrapper::returns::Failure: Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::returns::Renderer: Render<
                S,
                wrapper::returns::Input<'expansion, 'lowered, S>,
                Output = wrapper::returns::Tokens,
            >,
        wrapper::async_call::Renderer:
            Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
        wrapper::param::direct::Record:
            Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'lowered, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        match self.pair.binding() {
            RecordDecl::Direct(binding) => Direct {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .render(),
            RecordDecl::Encoded(binding) => Encoded {
                source: self.pair.source(),
                binding,
                expansion: self.expansion,
            }
            .render(),
            _ => Err(Error::UnsupportedExpansion("unknown record declaration")),
        }
    }
}

impl<'expansion, 'lowered, S> Direct<'expansion, 'lowered, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
{
    fn render(self) -> Result<TokenStream, Error> {
        let record = record_ident(self.source)?;
        let size = layout_size(self.binding.layout().size().get())?;
        let alignment = layout_size(self.binding.layout().alignment().get())?;
        let exports = RecordExports {
            source: self.source,
            record: record.clone(),
            initializers: self.binding.initializers(),
            methods: self.binding.methods(),
            receiver: ReceiverKind::Direct,
            expansion: self.expansion,
        }
        .render()?;
        Ok(quote! {
            const _: [(); #size] = [(); ::core::mem::size_of::<#record>()];
            const _: [(); #alignment] = [(); ::core::mem::align_of::<#record>()];

            unsafe impl ::boltffi::__private::Passable for #record {
                type In = #record;
                type Out = #record;

                unsafe fn unpack(input: #record) -> Self {
                    input
                }

                fn pack(self) -> #record {
                    self
                }
            }

            unsafe impl ::boltffi::__private::wire::Blittable for #record {}

            impl ::boltffi::__private::wire::WireEncode for #record {
                const ENCODING_KIND: ::boltffi::__private::wire::WireEncodingKind =
                    ::boltffi::__private::wire::WireEncodingKind::Blittable;

                fn is_fixed_size() -> bool {
                    true
                }

                fn fixed_size() -> Option<usize> {
                    Some(::core::mem::size_of::<Self>())
                }

                fn wire_size(&self) -> usize {
                    ::core::mem::size_of::<Self>()
                }

                fn encode_to(&self, buffer: &mut [u8]) -> usize {
                    <Self as ::boltffi::__private::wire::Blittable>::encode_value(self, buffer)
                }
            }

            impl ::boltffi::__private::wire::WireDecode for #record {
                fn decode_from(buffer: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    match <Self as ::boltffi::__private::wire::Blittable>::decode_value(buffer) {
                        Some(value) => Ok((value, ::core::mem::size_of::<Self>())),
                        None => Err(::boltffi::__private::wire::DecodeError::BufferTooSmall),
                    }
                }
            }

            impl ::boltffi::__private::VecTransport for #record {
                fn pack_vec(values: Vec<#record>) -> ::boltffi::__private::FfiBuf {
                    ::boltffi::__private::FfiBuf::from_vec(values)
                }

                unsafe fn unpack_vec(pointer: *const u8, byte_len: usize) -> Vec<#record> {
                    if byte_len == 0 {
                        return Vec::new();
                    }
                    let element_count = byte_len / ::core::mem::size_of::<#record>();
                    unsafe {
                        ::core::slice::from_raw_parts(pointer as *const #record, element_count)
                    }
                    .to_vec()
                }
            }

            #exports
        })
    }
}

impl<'expansion, 'lowered, S> Encoded<'expansion, 'lowered, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
{
    fn render(self) -> Result<TokenStream, Error> {
        let record = record_ident(self.source)?;
        let fields = self.fields()?;
        let wire_sizes = fields
            .iter()
            .map(|field| &field.wire_size)
            .collect::<Vec<_>>();
        let encoders = fields
            .iter()
            .map(|field| &field.encode_to)
            .collect::<Vec<_>>();
        let decoders = fields
            .iter()
            .map(|field| &field.decode_from)
            .collect::<Vec<_>>();
        let initializers = fields
            .iter()
            .map(|field| &field.initializer)
            .collect::<Vec<_>>();
        let exports = RecordExports {
            source: self.source,
            record: record.clone(),
            initializers: self.binding.initializers(),
            methods: self.binding.methods(),
            receiver: ReceiverKind::Encoded {
                codec: self.binding.write(),
            },
            expansion: self.expansion,
        }
        .render()?;

        Ok(quote! {
            unsafe impl ::boltffi::__private::WirePassable for #record {}

            impl ::boltffi::__private::wire::WireEncode for #record {
                fn wire_size(&self) -> usize {
                    0 #(+ #wire_sizes)*
                }

                fn encode_to(&self, buffer: &mut [u8]) -> usize {
                    let mut __boltffi_offset = 0usize;
                    #(#encoders)*
                    __boltffi_offset
                }
            }

            impl ::boltffi::__private::wire::WireDecode for #record {
                fn decode_from(buffer: &[u8]) -> ::boltffi::__private::wire::DecodeResult<Self> {
                    let mut __boltffi_offset = 0usize;
                    #(#decoders)*
                    Ok((Self { #(#initializers),* }, __boltffi_offset))
                }
            }

            impl ::boltffi::__private::VecTransport for #record {
                fn pack_vec(values: Vec<#record>) -> ::boltffi::__private::FfiBuf {
                    ::boltffi::__private::FfiBuf::wire_encode(&values)
                }

                unsafe fn unpack_vec(pointer: *const u8, byte_len: usize) -> Vec<#record> {
                    let bytes = if byte_len == 0 {
                        &[]
                    } else {
                        unsafe { ::core::slice::from_raw_parts(pointer, byte_len) }
                    };
                    ::boltffi::__private::wire::decode(bytes)
                        .expect("wire decode failed in VecTransport::unpack_vec")
                }
            }

            #exports
        })
    }

    fn fields(&self) -> Result<Vec<EncodedFieldTokens>, Error> {
        if self.source.fields.len() != self.binding.fields().len() {
            return Err(Error::SourceSyntaxMismatch(
                "source and binding record field counts differ",
            ));
        }
        self.source
            .fields
            .iter()
            .zip(self.binding.fields())
            .map(|(source, binding)| {
                EncodedField {
                    source,
                    binding,
                    expansion: self.expansion,
                }
                .tokens()
            })
            .collect()
    }
}

impl<'expansion, 'lowered, S: Target> EncodedField<'expansion, 'lowered, S> {
    fn tokens(self) -> Result<EncodedFieldTokens, Error> {
        self.validate_key()?;
        let field = field_ident(self.source)?;
        let generated = names::RecordField::new(&field);
        let decoded = generated.decoded();
        let used = generated.used();
        let wire = generated.wire();
        let rust_type = rust_api::TypeTokens::new(&self.source.type_expr)?.into_type();
        let codec = self.binding.codec().write().root();
        encoded::require_runtime_wire(codec)?;
        let wire_size = self.wire_size(&field, &wire, codec)?;
        let encode_to = self.encode_to(&field, &wire, codec)?;
        let decode_from = self.decode_from(&field, &decoded, &used, &rust_type, codec)?;
        Ok(EncodedFieldTokens {
            wire_size,
            encode_to,
            decode_from,
            initializer: field,
        })
    }

    fn validate_key(&self) -> Result<(), Error> {
        let expected = FieldKey::Named(CanonicalName::from(&self.source.name));
        if self.binding.key() == &expected {
            return Ok(());
        }
        Err(Error::SourceSyntaxMismatch(
            "source and binding record field keys differ",
        ))
    }

    fn wire_size(
        &self,
        field: &Ident,
        wire: &Ident,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let conversion = encoded::BorrowedOutgoing::new(codec, self.expansion);
        if !conversion.has_custom_conversion()? {
            return Ok(quote! {
                ::boltffi::__private::wire::WireEncode::wire_size(&self.#field)
            });
        }
        let converted = conversion.convert(quote! { &self.#field })?;
        Ok(quote! {
            {
                let #wire = #converted;
                ::boltffi::__private::wire::WireEncode::wire_size(&#wire)
            }
        })
    }

    fn encode_to(
        &self,
        field: &Ident,
        wire: &Ident,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let conversion = encoded::BorrowedOutgoing::new(codec, self.expansion);
        let value = match conversion.has_custom_conversion()? {
            true => {
                let converted = conversion.convert(quote! { &self.#field })?;
                quote! {
                    let #wire = #converted;
                    let __boltffi_written =
                        ::boltffi::__private::wire::WireEncode::encode_to(
                            &#wire,
                            &mut buffer[__boltffi_offset..]
                        );
                }
            }
            false => quote! {
                let __boltffi_written =
                    ::boltffi::__private::wire::WireEncode::encode_to(
                        &self.#field,
                        &mut buffer[__boltffi_offset..]
                    );
            },
        };
        Ok(quote! {
            {
                #value
                __boltffi_offset += __boltffi_written;
            }
        })
    }

    fn decode_from(
        &self,
        field: &Ident,
        decoded: &Ident,
        used: &Ident,
        rust_type: &Type,
        codec: &CodecNode,
    ) -> Result<TokenStream, Error> {
        let incoming = encoded::Incoming::new(codec, self.expansion);
        let decoded_type = incoming
            .decoded_type()?
            .unwrap_or_else(|| quote! { #rust_type });
        let converted = incoming.convert(quote! { #decoded })?;
        let value = match converted.changed() {
            true if converted.fallible() => {
                let converted_value = converted.tokens();
                quote! {
                    match #converted_value {
                        Ok(value) => value,
                        Err(_) => {
                            return Err(::boltffi::__private::wire::DecodeError::InvalidValue(
                                ::boltffi::__private::wire::InvalidWireValue::CustomConversion
                            ));
                        }
                    }
                }
            }
            true => {
                let converted_value = converted.tokens();
                quote! { #converted_value }
            }
            false => quote! { #decoded },
        };
        Ok(quote! {
            let (#decoded, #used) =
                <#decoded_type as ::boltffi::__private::wire::WireDecode>::decode_from(
                    &buffer[__boltffi_offset..]
                )?;
            __boltffi_offset += #used;
            let #field: #rust_type = #value;
        })
    }
}

impl<'expansion, 'lowered, S> RecordExports<'expansion, 'lowered, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
{
    fn render(self) -> Result<TokenStream, Error> {
        let initializers = self
            .initializers
            .iter()
            .map(|initializer| {
                let source_method = self.source_method(initializer.name())?;
                RecordExport {
                    source: self.source,
                    record: self.record.clone(),
                    source_method,
                    symbol: initializer.symbol(),
                    callable: initializer.callable(),
                    receiver: ReceiverKind::None,
                    expansion: self.expansion,
                }
                .render()
            })
            .collect::<Result<Vec<_>, _>>()?;
        let methods = self
            .methods
            .iter()
            .map(|method| {
                let source_method = self.source_method(method.name())?;
                RecordExport {
                    source: self.source,
                    record: self.record.clone(),
                    source_method,
                    symbol: method.target(),
                    callable: method.callable(),
                    receiver: self.receiver,
                    expansion: self.expansion,
                }
                .render()
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(quote! {
            #(#initializers)*
            #(#methods)*
        })
    }

    fn source_method(&self, name: &CanonicalName) -> Result<&'lowered MethodDef, Error> {
        let binding_name = name.as_path_string();
        let matches = self
            .source
            .methods
            .iter()
            .filter(|method| method.name.as_path_string() == binding_name)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [method] => Ok(*method),
            [] => Err(Error::SourceSyntaxMismatch(
                "source record method is missing for binding method",
            )),
            _ => Err(Error::SourceSyntaxMismatch(
                "source record method name is ambiguous",
            )),
        }
    }
}

impl<'expansion, 'lowered, S> RecordExport<'expansion, 'lowered, S>
where
    S: Target,
    wrapper::arguments::SyncRenderer: Render<
            S,
            wrapper::arguments::Input<'expansion, 'lowered, S>,
            Output = wrapper::arguments::Tokens,
        >,
    wrapper::returns::Failure:
        Render<S, wrapper::returns::FailureInput<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::returns::Renderer: Render<
            S,
            wrapper::returns::Input<'expansion, 'lowered, S>,
            Output = wrapper::returns::Tokens,
        >,
    wrapper::async_call::Renderer:
        Render<S, wrapper::async_call::Input<'expansion, 'lowered, S>, Output = TokenStream>,
    wrapper::param::direct::Record:
        Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
    wrapper::param::encoded::Renderer: Render<
            S,
            wrapper::param::encoded::Input<'expansion, 'lowered, S>,
            Output = wrapper::param::Tokens,
        >,
{
    fn render(self) -> Result<TokenStream, Error> {
        let method = method_ident(self.source_method)?;
        let (receiver, rust_call) = self.receiver.render(
            self.source,
            &self.record,
            self.callable,
            self.callable.receiver(),
            method,
            self.expansion,
        )?;
        let source_signature = rust_api::Callable::record_method(self.source_method, self.source);
        if matches!(self.callable.execution(), ExecutionDecl::Asynchronous(_)) {
            return <wrapper::async_call::Renderer as Render<S, _>>::render(
                wrapper::async_call::Renderer,
                wrapper::async_call::Input::exported(
                    self.symbol,
                    self.callable,
                    source_signature,
                    rust_call,
                    receiver,
                    visibility(self.source_method)?,
                    self.expansion,
                ),
            );
        }
        export::Renderer::new(
            self.symbol,
            self.callable,
            source_signature,
            rust_call,
            receiver,
            visibility(self.source_method)?,
            self.expansion,
        )
        .render()
    }
}

impl<'receiver> ReceiverKind<'receiver> {
    fn render<'expansion, S>(
        self,
        source: &'receiver RecordDef,
        record: &Ident,
        callable: &'receiver ExportedCallable<S>,
        receive: Option<Receive>,
        method: Ident,
        expansion: &'expansion Expansion<'receiver, S>,
    ) -> Result<(export::ReceiverTokens, export::RustCall), Error>
    where
        S: Target,
        wrapper::returns::Failure: Render<
                S,
                wrapper::returns::FailureInput<'expansion, 'receiver, S>,
                Output = TokenStream,
            >,
        wrapper::param::direct::Record:
            Render<S, wrapper::param::direct::RecordInput, Output = wrapper::param::Tokens>,
        wrapper::param::encoded::Renderer: Render<
                S,
                wrapper::param::encoded::Input<'expansion, 'receiver, S>,
                Output = wrapper::param::Tokens,
            >,
    {
        match (self, receive) {
            (Self::None, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
            (Self::None, Some(_)) => Err(Error::SourceSyntaxMismatch(
                "initializer binding unexpectedly has a receiver",
            )),
            (Self::Direct, Some(receive)) => {
                if receive == Receive::ByMutRef
                    && matches!(S::DIRECT_RECORD_PARAMS, DirectRecordCrossing::Value)
                {
                    return Err(Error::UnsupportedExpansion(
                        "mutable direct record receiver without writeback",
                    ));
                }
                let rust_type = record_type(source)?;
                let receiver = names::Wrapper::new(method.span()).receiver();
                let requires_failure_return =
                    matches!(S::DIRECT_RECORD_PARAMS, DirectRecordCrossing::Pointer);
                let failure = match requires_failure_return {
                    true => <wrapper::returns::Failure as Render<S, _>>::render(
                        wrapper::returns::Failure,
                        wrapper::returns::FailureInput::new(
                            callable.returns(),
                            callable.error(),
                            expansion,
                        ),
                    )?,
                    false => TokenStream::new(),
                };
                let tokens = <wrapper::param::direct::Record as Render<S, _>>::render(
                    wrapper::param::direct::Record,
                    wrapper::param::direct::RecordInput::new(
                        receive,
                        rust_type,
                        receiver.clone(),
                        failure,
                    ),
                )?;
                Ok((
                    export::ReceiverTokens::new(
                        tokens.ffi_parameters().to_vec(),
                        tokens.conversions().to_vec(),
                        tokens.writebacks().to_vec(),
                        requires_failure_return,
                    ),
                    export::RustCall::method(receiver, method),
                ))
            }
            (Self::Direct, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
            (Self::Encoded { codec }, Some(receive)) => {
                if receive == Receive::ByMutRef {
                    return Err(Error::UnsupportedExpansion(
                        "mutable encoded record receiver without writeback",
                    ));
                }
                let source_type = TypeExpr::record(
                    source.id.clone(),
                    SourcePath::single(source.name.spelling()),
                );
                let receiver = names::Wrapper::new(method.span()).receiver();
                let failure = <wrapper::returns::Failure as Render<S, _>>::render(
                    wrapper::returns::Failure,
                    wrapper::returns::FailureInput::new(
                        callable.returns(),
                        callable.error(),
                        expansion,
                    ),
                )?;
                let tokens = <wrapper::param::encoded::Renderer as Render<S, _>>::render(
                    wrapper::param::encoded::Renderer,
                    wrapper::param::encoded::Input::new(
                        codec,
                        <S as SurfaceLower>::encoded_param_shape(),
                        rust_api::DecodeTarget::received(receive, &source_type)?,
                        receiver.clone(),
                        failure,
                        expansion,
                    ),
                )?;
                Ok((
                    export::ReceiverTokens::new(
                        tokens.ffi_parameters().to_vec(),
                        tokens.conversions().to_vec(),
                        tokens.writebacks().to_vec(),
                        true,
                    ),
                    export::RustCall::method(receiver, method),
                ))
            }
            (Self::Encoded { .. }, None) => Ok((
                export::ReceiverTokens::none(),
                export::RustCall::associated(quote! { #record }, method),
            )),
        }
    }
}

fn record_ident(source: &RecordDef) -> Result<Ident, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source record name is not a Rust identifier"))
}

fn method_ident(source: &MethodDef) -> Result<Ident, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source method name is not a Rust identifier"))
}

fn record_type(source: &RecordDef) -> Result<Type, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source record name is not a Rust type"))
}

fn field_ident(source: &FieldDef) -> Result<Ident, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source field name is not a Rust identifier"))
}

fn layout_size(bytes: u64) -> Result<usize, Error> {
    usize::try_from(bytes).map_err(|_| Error::SourceSyntaxMismatch("record layout is too large"))
}

fn visibility(method: &MethodDef) -> Result<TokenStream, Error> {
    match &method.source.visibility {
        Visibility::Private => Ok(TokenStream::new()),
        Visibility::Public => Ok(quote! { pub }),
        Visibility::Restricted(path) => {
            let path = parse_str::<Path>(path).map_err(|_| {
                Error::SourceSyntaxMismatch("source visibility path is not Rust path")
            })?;
            Ok(quote! { pub(in #path) })
        }
    }
}

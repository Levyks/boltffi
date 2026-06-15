use boltffi_ast::{ClassDef, StreamDef, TypeExpr};
use boltffi_binding::{
    ClassDecl, CodecNode, NativeSymbol, Op, StreamDecl, StreamItemPlan, TypeRef, ValueRef,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Type, parse_str};

use crate::experimental::{
    error::Error,
    expansion::{DeclarationPair, Expansion},
    rust_api,
    target::Target,
    wrapper::{self, Render, names},
};

pub struct Renderer<'expansion, 'lowered, S: Target> {
    stream: DeclarationPair<'lowered, StreamDef, StreamDecl<S>>,
    owner: DeclarationPair<'lowered, ClassDef, ClassDecl<S>>,
    expansion: &'expansion Expansion<'lowered, S>,
}

struct StreamSymbols<'lowered> {
    subscribe: &'lowered NativeSymbol,
    pop_batch: &'lowered NativeSymbol,
    wait: &'lowered NativeSymbol,
    poll: &'lowered NativeSymbol,
    unsubscribe: &'lowered NativeSymbol,
    free: &'lowered NativeSymbol,
}

struct SubscribeExport<'stream> {
    class: &'stream Ident,
    method: &'stream Ident,
    handle_type: &'stream Ident,
    receiver: &'stream Ident,
    receiver_handle: &'stream Ident,
    stream_handle_type: &'stream TokenStream,
    stream_handle_zero: &'stream TokenStream,
}

struct StreamItemType<'source> {
    source: &'source TypeExpr,
}

impl<'expansion, 'lowered, S: Target> Renderer<'expansion, 'lowered, S> {
    pub fn new(
        stream: DeclarationPair<'lowered, StreamDef, StreamDecl<S>>,
        owner: DeclarationPair<'lowered, ClassDef, ClassDecl<S>>,
        expansion: &'expansion Expansion<'lowered, S>,
    ) -> Self {
        Self {
            stream,
            owner,
            expansion,
        }
    }

    pub fn render(self) -> Result<TokenStream, Error>
    where
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
        wrapper::returns::encoded::Renderer: Render<
                S,
                wrapper::returns::encoded::Empty<S>,
                Output = wrapper::returns::encoded::Tokens,
            >,
        wrapper::returns::encoded::Renderer: for<'codec> Render<
                S,
                wrapper::returns::encoded::Input<'expansion, 'codec, 'lowered, S>,
                Output = wrapper::returns::encoded::Tokens,
            >,
    {
        self.validate_owner()?;
        let cfg = S::cfg_attr();
        let class = class_ident(self.owner.source())?;
        let method = method_ident(self.stream.source())?;
        let handle_type = names::Class::new(&class).handle();
        let wrapper_names = names::Wrapper::new(method.span());
        let receiver = wrapper_names.receiver();
        let receiver_handle = names::Parameter::new(&receiver).handle();
        let item_type = StreamItemType::new(&self.stream.source().item_type).into_type()?;
        let stream_handle = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.stream.binding().handle()),
        )?;
        let stream_handle_type = stream_handle.ty();
        let stream_handle_zero = stream_handle.zero();
        let symbols = StreamSymbols::new(self.stream.binding().protocol());
        let subscribe = self.subscribe(SubscribeExport {
            class: &class,
            method: &method,
            handle_type: &handle_type,
            receiver: &receiver,
            receiver_handle: &receiver_handle,
            stream_handle_type,
            stream_handle_zero,
        })?;
        let pop_batch = self.pop_batch(
            &item_type,
            stream_handle_type,
            stream_handle_zero,
            &wrapper_names.stream_items(),
            &wrapper_names.stream_output_slots(),
        )?;
        let wait = symbols.wait();
        let poll = symbols.poll();
        let unsubscribe = symbols.unsubscribe();
        let free = symbols.free();

        Ok(quote! {
            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #subscribe

            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #pop_batch

            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #wait(
                subscription_handle: #stream_handle_type,
                timeout_milliseconds: u32,
            ) -> i32 {
                if subscription_handle == #stream_handle_zero {
                    return ::boltffi::__private::WaitResult::Unsubscribed as i32;
                }
                let subscription = unsafe {
                    &*(subscription_handle as usize as *const ::boltffi::__private::EventSubscription<#item_type>)
                };
                subscription.wait_for_events(timeout_milliseconds) as i32
            }

            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #poll(
                subscription_handle: #stream_handle_type,
                callback_data: u64,
                callback: ::boltffi::__private::StreamContinuationCallback,
            ) {
                if subscription_handle == #stream_handle_zero {
                    callback(callback_data, ::boltffi::__private::StreamPollResult::Closed);
                    return;
                }
                let subscription = unsafe {
                    &*(subscription_handle as usize as *const ::boltffi::__private::EventSubscription<#item_type>)
                };
                subscription.poll(callback_data, callback);
            }

            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #unsubscribe(
                subscription_handle: #stream_handle_type,
            ) {
                if subscription_handle == #stream_handle_zero {
                    return;
                }
                let subscription = unsafe {
                    &*(subscription_handle as usize as *const ::boltffi::__private::EventSubscription<#item_type>)
                };
                subscription.unsubscribe();
            }

            #cfg
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #free(
                subscription_handle: #stream_handle_type,
            ) {
                if subscription_handle == #stream_handle_zero {
                    return;
                }
                drop(unsafe {
                    ::std::sync::Arc::from_raw(
                        subscription_handle as usize as *const ::boltffi::__private::EventSubscription<#item_type>
                    )
                });
            }
        })
    }

    fn validate_owner(&self) -> Result<(), Error> {
        if self.stream.source().owner.as_ref() != Some(&self.owner.source().id) {
            return Err(Error::SourceSyntaxMismatch(
                "source stream owner does not match source class",
            ));
        }
        if self.stream.binding().owner() != Some(self.owner.binding().id()) {
            return Err(Error::SourceSyntaxMismatch(
                "lowered stream owner does not match lowered class",
            ));
        }
        Ok(())
    }

    fn subscribe(&self, subscribe: SubscribeExport<'_>) -> Result<TokenStream, Error>
    where
        wrapper::handle::Carrier: Render<
                S,
                wrapper::handle::CarrierInput<S::HandleCarrier>,
                Output = wrapper::handle::CarrierTokens,
            >,
    {
        let symbol = StreamSymbols::new(self.stream.binding().protocol()).subscribe();
        let carrier = <wrapper::handle::Carrier as Render<S, _>>::render(
            wrapper::handle::Carrier,
            wrapper::handle::CarrierInput::new(self.owner.binding().handle()),
        )?;
        let ffi_type = carrier.ty();
        let zero = carrier.zero();
        let SubscribeExport {
            class,
            method,
            handle_type,
            receiver,
            receiver_handle,
            stream_handle_type,
            stream_handle_zero,
        } = subscribe;
        Ok(quote! {
            #symbol(
                #receiver: #ffi_type,
            ) -> #stream_handle_type {
                if #receiver == #zero {
                    return #stream_handle_zero;
                }
                let #receiver_handle = #receiver as usize as *mut #handle_type;
                let #receiver: &#class = unsafe {
                    #handle_type::shared(#receiver_handle)
                };
                let subscription = #receiver.#method();
                ::std::sync::Arc::into_raw(subscription) as usize as #stream_handle_type
            }
        })
    }

    fn pop_batch(
        &self,
        item_type: &Type,
        stream_handle_type: &TokenStream,
        stream_handle_zero: &TokenStream,
        items: &Ident,
        output_slots: &Ident,
    ) -> Result<TokenStream, Error>
    where
        wrapper::returns::encoded::Renderer: Render<
                S,
                wrapper::returns::encoded::Empty<S>,
                Output = wrapper::returns::encoded::Tokens,
            >,
        wrapper::returns::encoded::Renderer: for<'codec> Render<
                S,
                wrapper::returns::encoded::Input<'expansion, 'codec, 'lowered, S>,
                Output = wrapper::returns::encoded::Tokens,
            >,
    {
        let symbol = StreamSymbols::new(self.stream.binding().protocol()).pop_batch();
        match self.stream.binding().item() {
            StreamItemPlan::Direct { ty, .. } => {
                let body = match ty {
                    TypeRef::Primitive(_) | TypeRef::Record(_) => quote! {
                        fn __boltffi_pop_direct_stream_batch<StreamItem>(
                            subscription: &::boltffi::__private::EventSubscription<StreamItem>,
                            output_ptr: *mut <StreamItem as ::boltffi::__private::Passable>::Out,
                            output_capacity: usize,
                        ) -> usize
                        where
                            StreamItem:
                                ::boltffi::__private::Passable<Out = StreamItem> + Send + 'static,
                        {
                            let #output_slots = unsafe {
                                ::core::slice::from_raw_parts_mut(
                                    output_ptr.cast::<::core::mem::MaybeUninit<StreamItem>>(),
                                    output_capacity,
                                )
                            };
                            subscription.pop_batch_into(#output_slots)
                        }

                        __boltffi_pop_direct_stream_batch::<#item_type>(
                            subscription,
                            output_ptr,
                            output_capacity,
                        )
                    },
                    _ => quote! {
                        let #output_slots = unsafe {
                            ::core::slice::from_raw_parts_mut(
                                output_ptr as *mut ::core::mem::MaybeUninit<
                                    <#item_type as ::boltffi::__private::Passable>::Out
                                >,
                                output_capacity,
                            )
                        };

                        #output_slots
                            .iter_mut()
                            .map_while(|slot| {
                                let item = subscription.pop_event()?;
                                slot.write(<#item_type as ::boltffi::__private::Passable>::pack(item));
                                Some(())
                            })
                            .count()
                    },
                };
                Ok(quote! {
                    #symbol(
                        subscription_handle: #stream_handle_type,
                        output_ptr: *mut <#item_type as ::boltffi::__private::Passable>::Out,
                        output_capacity: usize,
                    ) -> usize {
                        if subscription_handle == #stream_handle_zero || output_ptr.is_null() || output_capacity == 0 {
                            return 0;
                        }
                        let subscription = unsafe {
                            &*(subscription_handle as usize as *const ::boltffi::__private::EventSubscription<#item_type>)
                        };
                        #body
                    }
                })
            }
            StreamItemPlan::Encoded { read, shape, .. } => {
                let empty = <wrapper::returns::encoded::Renderer as Render<S, _>>::render(
                    wrapper::returns::encoded::Renderer,
                    wrapper::returns::encoded::Empty::new(*shape),
                )?;
                let batch_codec = CodecNode::Sequence {
                    len: Op::sequence_len(ValueRef::self_value()),
                    element: Box::new(read.root().clone()),
                };
                let value = <wrapper::returns::encoded::Renderer as Render<S, _>>::render(
                    wrapper::returns::encoded::Renderer,
                    wrapper::returns::encoded::Input::root(
                        &batch_codec,
                        *shape,
                        items.clone(),
                        self.expansion,
                    ),
                )?;
                let return_type = empty.return_type();
                let empty_value = empty.value();
                let batch_value = value.value();
                Ok(quote! {
                    #symbol(
                        subscription_handle: #stream_handle_type,
                        max_count: usize,
                    ) #return_type {
                        if subscription_handle == #stream_handle_zero || max_count == 0 {
                            return #empty_value;
                        }
                        let subscription = unsafe {
                            &*(subscription_handle as usize as *const ::boltffi::__private::EventSubscription<#item_type>)
                        };
                        let #items: Vec<#item_type> = ::core::iter::from_fn(|| subscription.pop_event())
                            .take(max_count)
                            .collect();

                        if #items.is_empty() {
                            #empty_value
                        } else {
                            #batch_value
                        }
                    }
                })
            }
            _ => Err(Error::UnsupportedExpansion("unknown stream item plan")),
        }
    }
}

impl<'source> StreamItemType<'source> {
    const fn new(source: &'source TypeExpr) -> Self {
        Self { source }
    }

    fn into_type(self) -> Result<Type, Error> {
        rust_api::TypeTokens::new(&Self::owned(self.source)).map(|tokens| tokens.into_type())
    }

    fn owned(source: &TypeExpr) -> TypeExpr {
        match source {
            TypeExpr::Str => TypeExpr::String,
            TypeExpr::Slice(element) | TypeExpr::Vec(element) => {
                TypeExpr::vec(Self::owned(element))
            }
            TypeExpr::Option(inner) => TypeExpr::option(Self::owned(inner)),
            TypeExpr::Result { ok, err } => TypeExpr::result(Self::owned(ok), Self::owned(err)),
            TypeExpr::Tuple(elements) => {
                TypeExpr::tuple(elements.iter().map(Self::owned).collect())
            }
            TypeExpr::Map { kind, key, value } => {
                TypeExpr::map(*kind, Self::owned(key), Self::owned(value))
            }
            _ => source.clone(),
        }
    }
}

impl<'lowered> StreamSymbols<'lowered> {
    fn new(protocol: &'lowered boltffi_binding::StreamProtocol) -> Self {
        Self {
            subscribe: protocol.subscribe(),
            pop_batch: protocol.pop_batch(),
            wait: protocol.wait(),
            poll: protocol.poll(),
            unsubscribe: protocol.unsubscribe(),
            free: protocol.free(),
        }
    }

    fn subscribe(&self) -> Ident {
        symbol_ident(self.subscribe)
    }

    fn pop_batch(&self) -> Ident {
        symbol_ident(self.pop_batch)
    }

    fn wait(&self) -> Ident {
        symbol_ident(self.wait)
    }

    fn poll(&self) -> Ident {
        symbol_ident(self.poll)
    }

    fn unsubscribe(&self) -> Ident {
        symbol_ident(self.unsubscribe)
    }

    fn free(&self) -> Ident {
        symbol_ident(self.free)
    }
}

fn class_ident(source: &ClassDef) -> Result<Ident, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source class name is not a Rust identifier"))
}

fn method_ident(source: &StreamDef) -> Result<Ident, Error> {
    parse_str(source.name.spelling())
        .map_err(|_| Error::SourceSyntaxMismatch("source stream name is not a Rust identifier"))
}

fn symbol_ident(symbol: &NativeSymbol) -> Ident {
    format_ident!("{}", symbol.name().as_str())
}

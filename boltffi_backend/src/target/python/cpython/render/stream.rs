use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ByteSize, Native, NativeSymbol, Primitive, ReadPlan, StreamDecl, StreamItemPlan,
    StreamItemPlanRender, TypeRef, native,
};

use crate::{
    bridge::python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{direct, primitive, result},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/stream.c", escape = "none")]
struct Template {
    subscribe: Method,
    pop_batch: Method,
    wait: Method,
    unsubscribe: Method,
    free: Method,
    item: Item,
    stream_handle_type: String,
    stream_handle_parser: &'static str,
    stream_handle_boxer: &'static str,
    subscribe_arity: usize,
    receiver: Option<Receiver>,
}

pub struct Stream {
    subscribe: Method,
    pop_batch: Method,
    wait: Method,
    unsubscribe: Method,
    free: Method,
    item: Item,
    handle: primitive::Runtime,
    receiver: Option<Receiver>,
}

impl Stream {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::new(declaration);
        Ok(Self {
            subscribe: Method::new(
                symbols.subscribe(),
                declaration.protocol().subscribe(),
                MethodFlags::FastCall,
                bridge,
            )?,
            pop_batch: Method::new(
                symbols.pop_batch(),
                declaration.protocol().pop_batch(),
                MethodFlags::FastCall,
                bridge,
            )?,
            wait: Method::new(
                symbols.wait(),
                declaration.protocol().wait(),
                MethodFlags::FastCall,
                bridge,
            )?,
            unsubscribe: Method::new(
                symbols.unsubscribe(),
                declaration.protocol().unsubscribe(),
                MethodFlags::FastCall,
                bridge,
            )?,
            free: Method::new(
                symbols.free(),
                declaration.protocol().free(),
                MethodFlags::FastCall,
                bridge,
            )?,
            item: Item::new(declaration.item(), bridge, context)?,
            handle: primitive::Runtime::native_handle(declaration.handle())?,
            receiver: declaration
                .owner()
                .map(|owner| Receiver::new(owner, context))
                .transpose()?,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        let stream_handle_type = self.handle.c_type()?.to_owned();
        let stream_handle_parser = self.handle.parser()?;
        let stream_handle_boxer = self.handle.boxer()?;
        let source = Template {
            subscribe: self.subscribe,
            pop_batch: self.pop_batch,
            wait: self.wait,
            unsubscribe: self.unsubscribe,
            free: self.free,
            item: self.item,
            stream_handle_type,
            stream_handle_parser,
            stream_handle_boxer,
            subscribe_arity: self.receiver.iter().count(),
            receiver: self.receiver,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        [
            &self.subscribe.method,
            &self.pop_batch.method,
            &self.wait.method,
            &self.unsubscribe.method,
            &self.free.method,
        ]
        .into_iter()
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        [
            Some(self.handle),
            Some(primitive::Runtime::new(Primitive::U32)),
            Some(primitive::Runtime::new(Primitive::USize)),
            self.receiver.as_ref().map(|receiver| receiver.primitive),
            self.item.primitive,
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    pub fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        self.item.owned_buffer()
    }
}

#[derive(Clone)]
struct Receiver {
    primitive: primitive::Runtime,
    handle_type: String,
    parser: &'static str,
}

impl Receiver {
    fn new(owner: boltffi_binding::ClassId, context: &RenderContext<Native>) -> Result<Self> {
        let handle =
            context
                .class(owner)
                .map(|class| class.handle())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "stream owner without class declaration",
                })?;
        let handle = primitive::Runtime::native_handle(handle)?;
        Ok(Self {
            primitive: handle,
            handle_type: handle.c_type()?,
            parser: handle.parser()?,
        })
    }
}

struct Method {
    python_name: String,
    wrapper: String,
    storage: String,
    method: ExtensionMethod,
}

impl Method {
    fn new(
        python_name: String,
        symbol: &NativeSymbol,
        flags: MethodFlags,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        let loaded = bridge
            .loaded_function(symbol)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "stream method without C bridge symbol",
            })?;
        let wrapper = format!("boltffi_python_stream_wrapper_{}", symbol.name().as_str());
        let method = ExtensionMethod::new(python_name.clone(), wrapper.clone(), flags)?;
        Ok(Self {
            python_name,
            wrapper,
            storage: loaded.storage_name().to_owned(),
            method,
        })
    }
}

struct Item {
    kind: ItemKind,
    primitive: Option<primitive::Runtime>,
}

impl Item {
    fn new(
        plan: &StreamItemPlan<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut StreamItem { bridge, context })
    }

    fn is_direct(&self) -> bool {
        matches!(self.kind, ItemKind::Direct { .. })
    }

    fn c_type(&self) -> &str {
        match &self.kind {
            ItemKind::Direct { c_type, .. } => c_type,
            ItemKind::Encoded => "",
        }
    }

    fn boxer(&self) -> &str {
        match &self.kind {
            ItemKind::Direct { boxer, .. } => boxer,
            ItemKind::Encoded => "",
        }
    }

    fn owned_buffer(&self) -> Option<result::OwnedBuffer> {
        match self.kind {
            ItemKind::Direct { .. } => None,
            ItemKind::Encoded => Some(result::OwnedBuffer::RawWire),
        }
    }

    fn direct(
        ty: &TypeRef,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_type_ref(
            ty,
            bridge,
            context,
            "unsupported direct stream item",
        )?;
        Ok(Self {
            kind: ItemKind::Direct {
                c_type: direct.c_type().to_owned(),
                boxer: direct.boxer().to_owned(),
            },
            primitive: direct.primitive(),
        })
    }
}

struct StreamItem<'bridge, 'context, 'bindings> {
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> StreamItemPlanRender<'plan, Native> for StreamItem<'_, '_, '_> {
    type Output = Result<Item>;

    fn direct(&mut self, ty: &'plan TypeRef, _: ByteSize) -> Self::Output {
        Item::direct(ty, self.bridge, self.context)
    }

    fn encoded(
        &mut self,
        _: &'plan TypeRef,
        _: &'plan ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => Ok(Item {
                kind: ItemKind::Encoded,
                primitive: None,
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded stream item shape",
            }),
        }
    }
}

enum ItemKind {
    Direct { c_type: String, boxer: String },
    Encoded,
}

pub struct Symbols {
    stem: String,
}

impl Symbols {
    pub fn new(declaration: &StreamDecl<Native>) -> Self {
        Self {
            stem: Name::new(declaration.name()).function(),
        }
    }

    pub fn subscribe(&self) -> String {
        self.stem.clone()
    }

    pub fn pop_batch(&self) -> String {
        format!("{}_pop_batch", self.stem)
    }

    pub fn wait(&self) -> String {
        format!("{}_wait", self.stem)
    }

    pub fn unsubscribe(&self) -> String {
        format!("{}_unsubscribe", self.stem)
    }

    pub fn free(&self) -> String {
        format!("{}_free", self.stem)
    }
}

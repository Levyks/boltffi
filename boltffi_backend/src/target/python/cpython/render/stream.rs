use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClassDecl, DeclarationRef, Native, NativeSymbol, Primitive, StreamDecl, StreamItemPlan,
    StreamMode, TypeRef,
};

use crate::{
    bridge::python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{enumeration, primitive, record, scalar_handle},
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

pub struct Wrapper {
    subscribe: Method,
    pop_batch: Method,
    wait: Method,
    unsubscribe: Method,
    free: Method,
    item: Item,
    handle: scalar_handle::Carrier,
    receiver: Option<Receiver>,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &StreamDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if matches!(declaration.mode(), StreamMode::Callback) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback-mode stream",
            });
        }
        let StreamItemPlan::Direct { ty, .. } = declaration.item() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded stream item",
            });
        };
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
            item: Item::new(ty, bridge, context)?,
            handle: scalar_handle::Carrier::new(declaration.handle())?,
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
            Some(self.handle.primitive()),
            Some(primitive::Runtime::new(Primitive::U32)),
            Some(primitive::Runtime::new(Primitive::USize)),
            self.receiver.as_ref().map(|receiver| receiver.primitive),
            self.item.primitive,
        ]
        .into_iter()
        .flatten()
        .collect()
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
        let handle = context
            .bindings()
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Class(class) if class.id() == owner => Some(class),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(ClassDecl::handle)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "stream owner without class declaration",
            })?;
        let handle = scalar_handle::Carrier::new(handle)?;
        Ok(Self {
            primitive: handle.primitive(),
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
    c_type: String,
    boxer: String,
    primitive: Option<primitive::Runtime>,
}

impl Item {
    fn new(
        ty: &TypeRef,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match ty {
            TypeRef::Primitive(primitive) => {
                let primitive = primitive::Runtime::new(*primitive);
                Ok(Self {
                    c_type: primitive.c_type()?,
                    boxer: primitive.boxer()?.to_owned(),
                    primitive: Some(primitive),
                })
            }
            TypeRef::Record(record) => {
                let symbols = record::Symbols::from_record_id(*record, bridge, context)?;
                Ok(Self {
                    c_type: symbols.c_type().to_owned(),
                    boxer: symbols.boxer().to_owned(),
                    primitive: None,
                })
            }
            TypeRef::Enum(enumeration) => {
                let symbols = enumeration::Symbols::from_enum_id(*enumeration, bridge, context)?;
                Ok(Self {
                    c_type: symbols.c_type().to_owned(),
                    boxer: symbols.boxer().to_owned(),
                    primitive: None,
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported direct stream item",
            }),
        }
    }
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

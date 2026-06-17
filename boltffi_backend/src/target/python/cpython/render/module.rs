use std::collections::BTreeSet;

use askama::Template as AskamaTemplate;
use boltffi_binding::{DeclarationRef, Native};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{
        Emitted, Error, FileLayout, GeneratedOutput, RenderContext, RenderedDeclaration, Result,
    },
    target::python::cpython::render::{
        callback, class, direct_vector, enumeration, function, method, primitive, record, result,
        stream,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/native_module.c", escape = "none")]
struct NativeModuleTemplate {
    module_name: String,
    method_table: String,
    module_definition: String,
    free_function: String,
    init_function: String,
    support: ModuleSupport,
    records: Vec<String>,
    enums: Vec<String>,
    classes: Vec<String>,
    callbacks: Vec<String>,
    streams: Vec<String>,
    host_bindings: Vec<String>,
    functions: Vec<String>,
    methods: Vec<method::Entry>,
    cleanup: Vec<String>,
}

pub struct NativeModule<'bridge, 'context, 'decl> {
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'context, Native>,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

impl<'bridge, 'context, 'decl> NativeModule<'bridge, 'context, 'decl> {
    pub fn new(
        bridge: &'bridge PythonCExtBridgeContract,
        context: &'context RenderContext<'context, Native>,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self {
            bridge,
            context,
            declarations,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let bridge = self.bridge;
        let records = self.records()?;
        let enums = self.enums()?;
        let classes = self.classes()?;
        let callbacks = self.callbacks()?;
        let streams = self.streams()?;
        let functions = self.functions()?;
        let methods = self
            .bridge
            .methods()
            .iter()
            .chain(records.iter().flat_map(|record| record.wrapper.methods()))
            .chain(
                enums
                    .iter()
                    .flat_map(|enumeration| enumeration.wrapper.methods()),
            )
            .chain(classes.iter().flat_map(|class| class.wrapper.methods()))
            .chain(streams.iter().flat_map(|stream| stream.wrapper.methods()))
            .chain(functions.iter().map(|function| function.wrapper.method()))
            .map(method::Entry::from_method)
            .collect();
        let support = ModuleSupport::new(
            bridge,
            records.iter().map(|record| &record.wrapper),
            enums.iter().map(|enumeration| &enumeration.wrapper),
            classes.iter().map(|class| &class.wrapper),
            callbacks.iter().map(|callback| &callback.wrapper),
            streams.iter().map(|stream| &stream.wrapper),
            functions.iter().map(|function| &function.wrapper),
        )?;
        let source = NativeModuleTemplate {
            module_name: bridge.module().as_str().to_owned(),
            method_table: bridge.symbols().method_table().to_owned(),
            module_definition: bridge.symbols().module_definition().to_owned(),
            free_function: bridge.symbols().free_function().to_owned(),
            init_function: bridge.symbols().init_function().to_owned(),
            support,
            records: records.iter().map(|record| record.source.clone()).collect(),
            enums: enums
                .iter()
                .map(|enumeration| enumeration.source.clone())
                .collect(),
            classes: classes.iter().map(|class| class.source.clone()).collect(),
            callbacks: callbacks
                .iter()
                .map(|callback| callback.source.clone())
                .collect(),
            streams: streams.iter().map(|stream| stream.source.clone()).collect(),
            host_bindings: callbacks
                .iter()
                .map(|callback| callback.wrapper.binding().to_owned())
                .collect(),
            functions: functions
                .into_iter()
                .map(|function| function.source)
                .collect(),
            methods,
            cleanup: records
                .iter()
                .map(|record| record.wrapper.cleanup())
                .chain(
                    enums
                        .iter()
                        .map(|enumeration| enumeration.wrapper.cleanup()),
                )
                .collect(),
        }
        .render()?;
        FileLayout::single(bridge.source_path().clone()).assemble([Emitted::primary(source)])
    }

    fn records(&self) -> Result<Vec<RenderedRecord>> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Record(record) => Some((record, declaration.emitted())),
                DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|(record, emitted)| {
                let wrapper = record::Wrapper::from_declaration(record, self.bridge, self.context)?;
                Ok(RenderedRecord {
                    wrapper,
                    source: emitted.primary_chunk().as_str().to_owned(),
                })
            })
            .collect()
    }

    fn functions(&self) -> Result<Vec<RenderedFunction>> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Function(function) => Some((function, declaration.emitted())),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .filter(|(function, _)| function::Wrapper::supports(function.callable()))
            .map(|(function, emitted)| {
                let wrapper =
                    function::Wrapper::from_declaration(function, self.bridge, self.context)?;
                Ok(RenderedFunction {
                    wrapper,
                    source: emitted.primary_chunk().as_str().to_owned(),
                })
            })
            .collect()
    }

    fn enums(&self) -> Result<Vec<RenderedEnum>> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Enum(enumeration) => Some((enumeration, declaration.emitted())),
                DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|(enumeration, emitted)| {
                let wrapper =
                    enumeration::Wrapper::from_declaration(enumeration, self.bridge, self.context)?;
                Ok(RenderedEnum {
                    wrapper,
                    source: emitted.primary_chunk().as_str().to_owned(),
                })
            })
            .collect()
    }

    fn classes(&self) -> Result<Vec<RenderedClass>> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Class(class) => Some((class, declaration.emitted())),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|(declaration, emitted)| {
                let wrapper =
                    class::Wrapper::from_declaration(declaration, self.bridge, self.context)?;
                Ok(RenderedClass {
                    wrapper,
                    source: emitted.primary_chunk().as_str().to_owned(),
                })
            })
            .collect()
    }

    fn callbacks(&self) -> Result<Vec<RenderedCallback>> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Callback(callback) => Some((callback, declaration.emitted())),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .filter(|(declaration, _)| callback::Wrapper::supports(declaration, self.bridge))
            .map(|(declaration, emitted)| {
                let wrapper = callback::Wrapper::from_declaration(declaration, self.bridge)?;
                Ok(RenderedCallback {
                    wrapper,
                    source: emitted.primary_chunk().as_str().to_owned(),
                })
            })
            .collect()
    }

    fn streams(&self) -> Result<Vec<RenderedStream>> {
        self.declarations
            .iter()
            .filter_map(|declaration| match declaration.declaration() {
                DeclarationRef::Stream(stream) => Some((stream, declaration.emitted())),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .filter(|(declaration, _)| stream::Wrapper::supports(declaration))
            .map(|(declaration, emitted)| {
                let wrapper =
                    stream::Wrapper::from_declaration(declaration, self.bridge, self.context)?;
                Ok(RenderedStream {
                    wrapper,
                    source: emitted.primary_chunk().as_str().to_owned(),
                })
            })
            .collect()
    }
}

struct RenderedFunction {
    wrapper: function::Wrapper,
    source: String,
}

struct RenderedRecord {
    wrapper: record::Wrapper,
    source: String,
}

struct RenderedEnum {
    wrapper: enumeration::Wrapper,
    source: String,
}

struct RenderedClass {
    wrapper: class::Wrapper,
    source: String,
}

struct RenderedCallback {
    wrapper: callback::Wrapper,
    source: String,
}

struct RenderedStream {
    wrapper: stream::Wrapper,
    source: String,
}

struct ModuleSupport {
    primitives: Vec<primitive::Support>,
    wire_primitives: Vec<primitive::Support>,
    owned_primitives: Vec<primitive::Support>,
    direct_vector_elements: Vec<direct_vector::Element>,
    free_buffer: String,
    string_arguments: bool,
    bytes_arguments: bool,
    raw_wire_arguments: bool,
    string_returns: bool,
    bytes_returns: bool,
    raw_wire_returns: bool,
    encoded_records: bool,
    data_enums: bool,
    record_types: bool,
    c_style_enums: bool,
}

impl ModuleSupport {
    fn new<'record, 'enumeration, 'class, 'callback, 'stream_wrapper, 'function>(
        bridge: &PythonCExtBridgeContract,
        records: impl Iterator<Item = &'record record::Wrapper>,
        enums: impl Iterator<Item = &'enumeration enumeration::Wrapper>,
        classes: impl Iterator<Item = &'class class::Wrapper>,
        callbacks: impl Iterator<Item = &'callback callback::Wrapper>,
        streams: impl Iterator<Item = &'stream_wrapper stream::Wrapper>,
        functions: impl Iterator<Item = &'function function::Wrapper>,
    ) -> Result<Self> {
        let records = records.collect::<Vec<_>>();
        let enums = enums.collect::<Vec<_>>();
        let classes = classes.collect::<Vec<_>>();
        let callbacks = callbacks.collect::<Vec<_>>();
        let streams = streams.collect::<Vec<_>>();
        let functions = functions.collect::<Vec<_>>();
        let encoded_records = records.iter().any(|record| record.needs_owned_buffer());
        let data_enums = enums
            .iter()
            .any(|enumeration| enumeration.needs_owned_buffer());
        let owned_buffers = functions
            .iter()
            .filter_map(|function| function.owned_buffer())
            .chain(records.iter().flat_map(|record| record.owned_buffers()))
            .chain(
                enums
                    .iter()
                    .flat_map(|enumeration| enumeration.owned_buffers()),
            )
            .chain(classes.iter().flat_map(|class| class.owned_buffers()))
            .chain(streams.iter().filter_map(|stream| stream.owned_buffer()))
            .collect::<BTreeSet<_>>();
        let direct_vector_elements = owned_buffers
            .iter()
            .filter_map(|buffer| match buffer {
                result::OwnedBuffer::DirectVector(element) => Some(element.clone()),
                result::OwnedBuffer::String
                | result::OwnedBuffer::Bytes
                | result::OwnedBuffer::RawWire
                | result::OwnedBuffer::Primitive(_) => None,
            })
            .chain(
                functions
                    .iter()
                    .flat_map(|function| function.direct_vector_elements()),
            )
            .chain(
                records
                    .iter()
                    .flat_map(|record| record.direct_vector_elements()),
            )
            .chain(
                enums
                    .iter()
                    .flat_map(|enumeration| enumeration.direct_vector_elements()),
            )
            .chain(
                classes
                    .iter()
                    .flat_map(|class| class.direct_vector_elements()),
            )
            .collect::<BTreeSet<_>>();
        let primitives = functions
            .iter()
            .flat_map(|function| function.primitives())
            .chain(classes.iter().flat_map(|class| class.primitives()))
            .chain(callbacks.iter().flat_map(|callback| callback.primitives()))
            .chain(streams.iter().flat_map(|stream| stream.primitives()))
            .chain(records.iter().flat_map(|record| record.primitives()))
            .chain(
                enums
                    .iter()
                    .filter_map(|enumeration| enumeration.primitive()),
            )
            .chain(
                direct_vector_elements
                    .iter()
                    .filter_map(direct_vector::Element::runtime_primitive),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect::<Result<Vec<_>>>()?;
        let wire_primitives = functions
            .iter()
            .flat_map(|function| function.wire_primitives())
            .chain(records.iter().flat_map(|record| record.wire_primitives()))
            .chain(
                enums
                    .iter()
                    .flat_map(|enumeration| enumeration.wire_primitives()),
            )
            .chain(classes.iter().flat_map(|class| class.wire_primitives()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect::<Result<Vec<_>>>()?;
        let owned_primitives = owned_buffers
            .iter()
            .filter_map(|buffer| match buffer {
                result::OwnedBuffer::Primitive(primitive) => Some(*primitive),
                result::OwnedBuffer::String
                | result::OwnedBuffer::Bytes
                | result::OwnedBuffer::RawWire
                | result::OwnedBuffer::DirectVector(_) => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect::<Result<Vec<_>>>()?;
        let string_arguments = functions
            .iter()
            .any(|function| function.has_string_argument())
            || records.iter().any(|record| record.has_string_argument())
            || enums
                .iter()
                .any(|enumeration| enumeration.has_string_argument())
            || classes.iter().any(|class| class.has_string_argument());
        let bytes_arguments = functions
            .iter()
            .any(|function| function.has_bytes_argument())
            || records.iter().any(|record| record.has_bytes_argument())
            || enums
                .iter()
                .any(|enumeration| enumeration.has_bytes_argument())
            || classes.iter().any(|class| class.has_bytes_argument());
        let raw_wire_arguments = functions
            .iter()
            .any(|function| function.has_raw_wire_argument())
            || records.iter().any(|record| record.has_raw_wire_argument())
            || enums
                .iter()
                .any(|enumeration| enumeration.has_raw_wire_argument())
            || classes.iter().any(|class| class.has_raw_wire_argument());
        Ok(Self {
            primitives,
            wire_primitives,
            owned_primitives,
            direct_vector_elements: direct_vector_elements.into_iter().collect(),
            free_buffer: Self::free_buffer_storage(bridge)?,
            string_arguments,
            bytes_arguments,
            raw_wire_arguments,
            string_returns: owned_buffers.contains(&result::OwnedBuffer::String),
            bytes_returns: owned_buffers.contains(&result::OwnedBuffer::Bytes),
            raw_wire_returns: owned_buffers.contains(&result::OwnedBuffer::RawWire),
            encoded_records,
            data_enums,
            record_types: !records.is_empty(),
            c_style_enums: !enums.is_empty(),
        })
    }

    fn primitives(&self) -> &[primitive::Support] {
        &self.primitives
    }

    fn wire_primitives(&self) -> &[primitive::Support] {
        &self.wire_primitives
    }

    fn owned_primitives(&self) -> &[primitive::Support] {
        &self.owned_primitives
    }

    fn direct_vector_elements(&self) -> &[direct_vector::Element] {
        &self.direct_vector_elements
    }

    fn free_buffer(&self) -> &str {
        &self.free_buffer
    }

    fn uses_wire_arguments(&self) -> bool {
        self.string_arguments
            || self.bytes_arguments
            || self.raw_wire_arguments
            || !self.wire_primitives.is_empty()
    }

    fn uses_owned_buffers(&self) -> bool {
        self.string_returns
            || self.bytes_returns
            || self.raw_wire_returns
            || !self.owned_primitives.is_empty()
            || !self.direct_vector_elements.is_empty()
            || self.encoded_records
            || self.data_enums
    }

    fn uses_wire_strings(&self) -> bool {
        self.string_arguments
    }

    fn uses_wire_bytes(&self) -> bool {
        self.bytes_arguments
    }

    fn uses_raw_wire_arguments(&self) -> bool {
        self.raw_wire_arguments
    }

    fn uses_owned_utf8(&self) -> bool {
        self.string_returns
    }

    fn uses_owned_bytes(&self) -> bool {
        self.bytes_returns
    }

    fn uses_owned_raw_wire(&self) -> bool {
        self.raw_wire_returns
    }

    fn uses_c_style_enums(&self) -> bool {
        self.c_style_enums
    }

    fn uses_registered_types(&self) -> bool {
        self.record_types || self.c_style_enums
    }

    fn free_buffer_storage(bridge: &PythonCExtBridgeContract) -> Result<String> {
        bridge
            .functions()
            .iter()
            .find(|function| function.function().name() == "boltffi_free_buf")
            .map(|function| function.storage_name().to_owned())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "missing CPython free buffer support symbol",
            })
    }
}

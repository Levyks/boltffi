use std::collections::BTreeSet;

use askama::Template as AskamaTemplate;
use boltffi_binding::{DeclarationRef, Native};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{
        Emitted, Error, FileLayout, GeneratedOutput, RenderContext, RenderedDeclaration, Result,
    },
    target::python::cpython::render::{enumeration, function, method, primitive, record, result},
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
        let functions = self.functions()?;
        let methods = self
            .bridge
            .methods()
            .iter()
            .chain(records.iter().map(|record| record.wrapper.method()))
            .chain(enums.iter().map(|enumeration| enumeration.wrapper.method()))
            .chain(functions.iter().map(|function| function.wrapper.method()))
            .map(method::Entry::from_method)
            .collect();
        let support = ModuleSupport::new(
            bridge,
            records.iter().map(|record| &record.wrapper),
            enums.iter().map(|enumeration| &enumeration.wrapper),
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
                let wrapper = record::Wrapper::from_declaration(record, self.bridge)?;
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
                let wrapper = enumeration::Wrapper::from_declaration(enumeration, self.bridge)?;
                Ok(RenderedEnum {
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

struct ModuleSupport {
    primitives: Vec<primitive::Support>,
    free_buffer: String,
    string_arguments: bool,
    bytes_arguments: bool,
    string_returns: bool,
    bytes_returns: bool,
    direct_records: bool,
    c_style_enums: bool,
}

impl ModuleSupport {
    fn new<'record, 'enumeration, 'function>(
        bridge: &PythonCExtBridgeContract,
        records: impl Iterator<Item = &'record record::Wrapper>,
        enums: impl Iterator<Item = &'enumeration enumeration::Wrapper>,
        functions: impl Iterator<Item = &'function function::Wrapper>,
    ) -> Result<Self> {
        let records = records.collect::<Vec<_>>();
        let enums = enums.collect::<Vec<_>>();
        let functions = functions.collect::<Vec<_>>();
        let primitives = functions
            .iter()
            .flat_map(|function| function.primitives())
            .chain(records.iter().flat_map(|record| record.primitives()))
            .chain(enums.iter().map(|enumeration| enumeration.primitive()))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(primitive::Support::new)
            .collect::<Result<Vec<_>>>()?;
        let owned_buffers = functions
            .iter()
            .filter_map(|function| function.owned_buffer())
            .collect::<BTreeSet<_>>();
        Ok(Self {
            primitives,
            free_buffer: Self::free_buffer_storage(bridge)?,
            string_arguments: functions
                .iter()
                .any(|function| function.has_string_argument()),
            bytes_arguments: functions
                .iter()
                .any(|function| function.has_bytes_argument()),
            string_returns: owned_buffers.contains(&result::OwnedBuffer::String),
            bytes_returns: owned_buffers.contains(&result::OwnedBuffer::Bytes),
            direct_records: !records.is_empty(),
            c_style_enums: !enums.is_empty(),
        })
    }

    fn primitives(&self) -> &[primitive::Support] {
        &self.primitives
    }

    fn free_buffer(&self) -> &str {
        &self.free_buffer
    }

    fn uses_wire_arguments(&self) -> bool {
        self.string_arguments || self.bytes_arguments
    }

    fn uses_owned_buffers(&self) -> bool {
        self.string_returns || self.bytes_returns
    }

    fn uses_wire_strings(&self) -> bool {
        self.string_arguments
    }

    fn uses_wire_bytes(&self) -> bool {
        self.bytes_arguments
    }

    fn uses_owned_utf8(&self) -> bool {
        self.string_returns
    }

    fn uses_owned_bytes(&self) -> bool {
        self.bytes_returns
    }

    fn uses_c_style_enums(&self) -> bool {
        self.c_style_enums
    }

    fn uses_registered_types(&self) -> bool {
        self.direct_records || self.c_style_enums
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

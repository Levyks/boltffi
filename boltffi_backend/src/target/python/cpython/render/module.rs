use std::collections::BTreeSet;

use askama::Template as AskamaTemplate;
use boltffi_binding::{DeclarationRef, Native};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Emitted, Error, FileLayout, GeneratedOutput, RenderedDeclaration, Result},
    target::python::cpython::render::{function, method, primitive, result},
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
    functions: Vec<String>,
    methods: Vec<method::Entry>,
}

pub struct NativeModule<'bridge, 'decl> {
    bridge: &'bridge PythonCExtBridgeContract,
    declarations: Vec<RenderedDeclaration<'decl, Native>>,
}

impl<'bridge, 'decl> NativeModule<'bridge, 'decl> {
    pub fn new(
        bridge: &'bridge PythonCExtBridgeContract,
        declarations: Vec<RenderedDeclaration<'decl, Native>>,
    ) -> Self {
        Self {
            bridge,
            declarations,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let bridge = self.bridge;
        let functions = self.functions()?;
        let methods = self
            .bridge
            .methods()
            .iter()
            .chain(functions.iter().map(|function| function.wrapper.method()))
            .map(method::Entry::from_method)
            .collect();
        let support = ModuleSupport::from_functions(
            bridge,
            functions.iter().map(|function| &function.wrapper),
        )?;
        let source = NativeModuleTemplate {
            module_name: bridge.module().as_str().to_owned(),
            method_table: bridge.symbols().method_table().to_owned(),
            module_definition: bridge.symbols().module_definition().to_owned(),
            free_function: bridge.symbols().free_function().to_owned(),
            init_function: bridge.symbols().init_function().to_owned(),
            support,
            functions: functions
                .into_iter()
                .map(|function| function.source)
                .collect(),
            methods,
        }
        .render()?;
        FileLayout::single(bridge.source_path().clone()).assemble([Emitted::primary(source)])
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
                let wrapper = function::Wrapper::from_declaration(function, self.bridge)?;
                Ok(RenderedFunction {
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

struct ModuleSupport {
    primitives: Vec<primitive::Support>,
    free_buffer: String,
    string_arguments: bool,
    bytes_arguments: bool,
    string_returns: bool,
    bytes_returns: bool,
}

impl ModuleSupport {
    fn from_functions<'function>(
        bridge: &PythonCExtBridgeContract,
        functions: impl Iterator<Item = &'function function::Wrapper>,
    ) -> Result<Self> {
        let functions = functions.collect::<Vec<_>>();
        let primitives = functions
            .iter()
            .flat_map(|function| function.primitives())
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

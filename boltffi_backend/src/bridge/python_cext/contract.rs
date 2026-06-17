use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use boltffi_binding::{Native, NativeSymbol, RecordId};

use crate::{
    bridge::c::{self, CBridgeContract, Function},
    core::{
        BridgeCapabilities, BridgeCapability, BridgeContract, Error, FilePath, Result,
        contract::sealed,
    },
};

/// Contract for the CPython C extension bridge layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct PythonCExtBridgeContract {
    capabilities: BridgeCapabilities,
    module: PythonExtensionName,
    source_path: FilePath,
    c_header: CHeaderInclude,
    symbols: ModuleSymbols,
    source_direct_records: BTreeMap<RecordId, c::Record>,
    functions: Vec<LoadedFunction>,
    loader: ExtensionMethod,
}

/// C include path used by the generated CPython source file.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct CHeaderInclude {
    path: String,
}

/// CPython extension module name used by `PyInit_<name>`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct PythonExtensionName {
    name: String,
}

/// C bridge function loaded into the CPython extension module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct LoadedFunction {
    function: Function,
    typedef_name: String,
    storage_name: String,
}

/// C identifiers reserved by the CPython extension module.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ModuleSymbols {
    init_function: String,
    method_table: String,
    module_definition: String,
    free_function: String,
}

/// CPython method table entry contributed by the bridge layer.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ExtensionMethod {
    python_name: String,
    c_function: String,
    flags: MethodFlags,
}

/// CPython call convention flags for one method table entry.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum MethodFlags {
    /// A method that receives one Python object argument.
    OneObject,
    /// A method that receives Python's fast-call argument array.
    FastCall,
    /// A method that receives no Python arguments.
    NoArgs,
}

impl PythonCExtBridgeContract {
    /// Builds the CPython bridge contract from the C bridge contract.
    pub fn from_c_bridge(
        module: PythonExtensionName,
        source_path: FilePath,
        c_bridge: &CBridgeContract,
    ) -> Result<Self> {
        let functions = c_bridge
            .support()
            .functions()
            .iter()
            .chain(
                c_bridge
                    .callbacks()
                    .iter()
                    .flat_map(|callback| [callback.register(), callback.create_handle()]),
            )
            .chain(c_bridge.functions())
            .map(LoadedFunction::new)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            capabilities: c_bridge
                .capabilities()
                .clone()
                .stable(BridgeCapability::PythonExtension),
            symbols: ModuleSymbols::new(&module)?,
            functions,
            loader: ExtensionMethod::loader()?,
            c_header: CHeaderInclude::from_files(&source_path, c_bridge.header_path())?,
            source_direct_records: c_bridge.source_direct_records().clone(),
            module,
            source_path,
        })
    }

    /// Returns the CPython extension module name.
    pub fn module(&self) -> &PythonExtensionName {
        &self.module
    }

    /// Returns the generated C extension source path.
    pub fn source_path(&self) -> &FilePath {
        &self.source_path
    }

    /// Returns the C header include path used by the extension source.
    pub fn c_header(&self) -> &CHeaderInclude {
        &self.c_header
    }

    /// Returns loaded C bridge functions.
    pub fn functions(&self) -> &[LoadedFunction] {
        &self.functions
    }

    /// Returns the loaded C bridge function for a native symbol.
    pub fn loaded_function(&self, symbol: &NativeSymbol) -> Option<&LoadedFunction> {
        self.functions
            .iter()
            .find(|function| function.function().name() == symbol.name().as_str())
    }

    /// Returns the C typedef selected for a direct source record.
    pub fn source_direct_record(&self, record: RecordId) -> Option<&c::Record> {
        self.source_direct_records.get(&record)
    }

    /// Returns C identifiers reserved by the extension module.
    pub fn symbols(&self) -> &ModuleSymbols {
        &self.symbols
    }

    /// Returns bridge-owned CPython methods.
    pub fn methods(&self) -> &[ExtensionMethod] {
        std::slice::from_ref(&self.loader)
    }

    /// Returns the bridge-owned loader method.
    pub fn loader_method(&self) -> &ExtensionMethod {
        &self.loader
    }
}

impl BridgeContract for PythonCExtBridgeContract {
    type Surface = Native;

    fn capabilities(&self) -> &BridgeCapabilities {
        &self.capabilities
    }
}

impl sealed::BridgeContract for PythonCExtBridgeContract {}

impl CHeaderInclude {
    /// Creates an include path relative to a generated CPython source file.
    pub fn from_files(source_path: &FilePath, header_path: &FilePath) -> Result<Self> {
        Self::new(Self::relative_to_source(
            source_path.as_path(),
            header_path.as_path(),
        ))
    }

    /// Returns the include path text.
    pub fn as_str(&self) -> &str {
        &self.path
    }

    fn new(path: PathBuf) -> Result<Self> {
        let path = path
            .as_os_str()
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::InvalidCIncludePath {
                path: path.display().to_string(),
            })?
            .replace('\\', "/");
        if path.is_empty() || path.bytes().any(|byte| matches!(byte, 0 | b'"')) {
            Err(Error::InvalidCIncludePath { path })
        } else {
            Ok(Self { path })
        }
    }

    fn relative_to_source(source_path: &Path, header_path: &Path) -> PathBuf {
        let Some(source_dir) = source_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        else {
            return header_path.to_path_buf();
        };
        if header_path.is_absolute() {
            return header_path.to_path_buf();
        }
        let source = Self::components(source_dir);
        let header = Self::components(header_path);
        let shared = source
            .iter()
            .zip(header.iter())
            .take_while(|(source, header)| source == header)
            .count();
        let mut relative = PathBuf::new();
        std::iter::repeat_with(|| OsString::from(".."))
            .take(source.len().saturating_sub(shared))
            .chain(header.into_iter().skip(shared))
            .for_each(|component| relative.push(component));
        relative
    }

    fn components(path: &Path) -> Vec<OsString> {
        path.components()
            .filter_map(|component| match component {
                Component::Normal(component) => Some(component.to_os_string()),
                Component::ParentDir => Some(OsString::from("..")),
                Component::CurDir | Component::RootDir | Component::Prefix(_) => None,
            })
            .collect()
    }
}

impl PythonExtensionName {
    /// Creates a CPython extension module name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        c::identifier::Identifier::parse(&name)?;
        Ok(Self { name })
    }

    /// Returns the module name text.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl LoadedFunction {
    /// Creates a loaded function from a C bridge function.
    pub fn new(function: &Function) -> Result<Self> {
        let symbol = c::identifier::Identifier::parse(function.name())?;
        Ok(Self {
            function: function.clone(),
            typedef_name: format!("boltffi_python_{}_fn", symbol.as_str()),
            storage_name: format!("boltffi_python_{}", symbol.as_str()),
        })
    }

    /// Returns the C bridge function.
    pub fn function(&self) -> &Function {
        &self.function
    }

    /// Returns the C function-pointer typedef name.
    pub fn typedef_name(&self) -> &str {
        &self.typedef_name
    }

    /// Returns the static function-pointer storage name.
    pub fn storage_name(&self) -> &str {
        &self.storage_name
    }
}

impl ModuleSymbols {
    /// Creates CPython extension module symbols.
    pub fn new(module: &PythonExtensionName) -> Result<Self> {
        let init_function = format!("PyInit_{}", module.as_str());
        let method_table = "boltffi_python_methods".to_owned();
        let module_definition = "boltffi_python_module".to_owned();
        let free_function = "boltffi_python_free_module".to_owned();
        [
            init_function.as_str(),
            method_table.as_str(),
            module_definition.as_str(),
            free_function.as_str(),
        ]
        .into_iter()
        .try_for_each(|symbol| c::identifier::Identifier::parse(symbol).map(|_| ()))?;
        Ok(Self {
            init_function,
            method_table,
            module_definition,
            free_function,
        })
    }

    /// Returns the `PyInit_*` function name.
    pub fn init_function(&self) -> &str {
        &self.init_function
    }

    /// Returns the CPython method table identifier.
    pub fn method_table(&self) -> &str {
        &self.method_table
    }

    /// Returns the CPython module definition identifier.
    pub fn module_definition(&self) -> &str {
        &self.module_definition
    }

    /// Returns the module cleanup function identifier.
    pub fn free_function(&self) -> &str {
        &self.free_function
    }
}

impl ExtensionMethod {
    /// Creates a CPython method table entry.
    pub fn new(
        python_name: impl Into<String>,
        c_function: impl Into<String>,
        flags: MethodFlags,
    ) -> Result<Self> {
        let python_name = python_name.into();
        if python_name.is_empty() || python_name.bytes().any(|byte| byte == 0) {
            return Err(Error::InvalidPythonMethodName { name: python_name });
        }
        let c_function = c_function.into();
        c::identifier::Identifier::parse(&c_function)?;
        Ok(Self {
            python_name,
            c_function,
            flags,
        })
    }

    /// Returns the Python-visible method name.
    pub fn python_name(&self) -> &str {
        &self.python_name
    }

    /// Returns the C function used by the method table entry.
    pub fn c_function(&self) -> &str {
        &self.c_function
    }

    /// Returns the CPython call convention flags.
    pub const fn flags(&self) -> MethodFlags {
        self.flags
    }

    fn loader() -> Result<Self> {
        Self::new(
            "_initialize_loader",
            "boltffi_python_initialize_loader",
            MethodFlags::OneObject,
        )
    }
}

impl MethodFlags {
    /// Returns the CPython C macro for these flags.
    pub const fn as_c_macro(self) -> &'static str {
        match self {
            Self::OneObject => "METH_O",
            Self::FastCall => "METH_FASTCALL",
            Self::NoArgs => "METH_NOARGS",
        }
    }
}

use std::fs;
use std::path::{Path, PathBuf};

use boltffi_backend::bridge::c::CBridge;
use boltffi_backend::bridge::python_cext::PythonCExtBridge;
use boltffi_backend::core::{BridgeLayer, bridge, host};
use boltffi_backend::target::python::PythonCExtHost;
use boltffi_backend::{GeneratedOutput, Target as BackendTarget};
use boltffi_binding::{BindingMetadataSurface, Bindings, Surface};
use thiserror::Error;

use crate::metadata::{BindingMetadataBuild, BindingMetadataBuildError};
use crate::target::Target;

/// Drives one BoltFFI generation from a compiled crate's embedded metadata
/// to rendered target-language files.
///
/// The driver runs the metadata build, selects the binding contract for the
/// target surface, renders it through the supplied [`Target`], and returns
/// the generated output. It carries no language-specific knowledge: the host
/// and bridge stack inside the [`Target`] decide everything about the
/// produced files.
#[derive(Clone, Debug)]
pub struct Generation {
    manifest_path: PathBuf,
    triple: Option<String>,
    python_module_name: Option<String>,
}

impl Generation {
    /// Creates a generation for a Cargo manifest.
    pub fn new(manifest_path: impl Into<PathBuf>) -> Self {
        Self {
            manifest_path: manifest_path.into(),
            triple: None,
            python_module_name: None,
        }
    }

    /// Builds for a Cargo target triple.
    pub fn triple(mut self, triple: impl Into<String>) -> Self {
        self.triple = Some(triple.into());
        self
    }

    /// Sets the CPython extension module name used by Python generation.
    pub fn python_module_name(mut self, module_name: impl Into<String>) -> Self {
        self.python_module_name = Some(module_name.into());
        self
    }

    /// Reads the embedded metadata, selects the target surface contract, and renders it.
    pub fn render(&self, target: Target) -> Result<GeneratedOutput, GenerationError> {
        match target {
            Target::Python => {
                let module_name = self.python_module_name.as_deref().unwrap_or("_native");
                let source_path = format!("{module_name}.c");
                let target = BackendTarget::new(
                    PythonCExtHost::new(),
                    BridgeLayer::new(
                        CBridge::default_header().map_err(GenerationError::Render)?,
                        PythonCExtBridge::new(module_name, source_path)
                            .map_err(GenerationError::Render)?,
                    ),
                );
                self.render_backend(&target)
            }
            Target::Swift
            | Target::Kotlin
            | Target::KotlinMultiplatform
            | Target::Java
            | Target::TypeScript
            | Target::Header
            | Target::Dart
            | Target::CSharp => Err(GenerationError::UnsupportedTarget { target }),
        }
    }

    /// Renders the bindings and writes every generated file under `output_dir`.
    pub fn write(
        &self,
        target: Target,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        let output = self.render(target)?;
        self.write_output(output, output_dir)
    }

    fn render_backend<H, S>(
        &self,
        target: &BackendTarget<H, S>,
    ) -> Result<GeneratedOutput, GenerationError>
    where
        H: host::HostBackend<Bridge = S::Contract, Surface = S::Surface>,
        S: bridge::BridgeStack,
    {
        let bindings = self.bindings::<S::Surface>()?;
        target.render(&bindings).map_err(GenerationError::Render)
    }

    fn write_output(
        &self,
        output: GeneratedOutput,
        output_dir: &Path,
    ) -> Result<Vec<PathBuf>, GenerationError> {
        output
            .files()
            .iter()
            .map(|file| {
                let path = output_dir.join(file.path().as_path());
                write_file(&path, file.contents())?;
                Ok(path)
            })
            .collect()
    }

    fn bindings<S: Surface>(&self) -> Result<Bindings<S>, GenerationError> {
        let surface = BindingMetadataSurface::from_target_triple(self.triple.as_deref());
        let mut build = BindingMetadataBuild::new(&self.manifest_path);
        if let Some(triple) = &self.triple {
            build = build.target(triple);
        }
        build
            .read()?
            .into_iter()
            .find(|envelope| envelope.surface() == surface)
            .and_then(|envelope| S::from_serialized(envelope.into_bindings()))
            .ok_or(GenerationError::MissingSurface { surface })
    }
}

/// Failure while generating bindings from embedded crate metadata.
#[derive(Debug, Error)]
pub enum GenerationError {
    /// The metadata build or artifact read failed.
    #[error(transparent)]
    Metadata(#[from] BindingMetadataBuildError),
    /// The compiled crate embedded no metadata for the requested surface.
    #[error("compiled crate embeds no binding metadata for the {surface:?} surface")]
    MissingSurface {
        /// Surface selected from the target triple.
        surface: BindingMetadataSurface,
    },
    /// The target backend failed to render the bindings.
    #[error("render bindings: {0}")]
    Render(boltffi_backend::Error),
    /// The target is not wired to the IR generation pipeline.
    #[error("IR generation is not available for {target}")]
    UnsupportedTarget {
        /// Requested target.
        target: Target,
    },
    /// A generated file could not be written to disk.
    #[error("write generated file `{path}`: {source}")]
    Write {
        /// Generated file path.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
}

fn write_file(path: &Path, contents: &str) -> Result<(), GenerationError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| GenerationError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, contents).map_err(|source| GenerationError::Write {
        path: path.to_path_buf(),
        source,
    })
}

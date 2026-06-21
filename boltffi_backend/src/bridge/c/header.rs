use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use boltffi_binding::{Bindings, Native};

use crate::core::{
    Emitted, Error, FileLayout, FilePath, GeneratedOutput, Result, bridge, contract::sealed,
};

use super::{contract::CBridgeContract, template};

/// C bridge backend for native bindings.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CBridge {
    path: FilePath,
}

/// C header include path written by a generated C source file.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct HeaderInclude {
    path: String,
}

impl CBridge {
    /// Creates a C header bridge.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            path: FilePath::new(path)?,
        })
    }

    /// Creates a C header bridge using `boltffi.h`.
    pub fn default_header() -> Result<Self> {
        Self::new("boltffi.h")
    }

    /// Returns the generated header path.
    pub fn path(&self) -> &FilePath {
        &self.path
    }
}

impl bridge::BridgeBackend for CBridge {
    type Surface = Native;
    type Input = Bindings<Native>;
    type Contract = CBridgeContract;
    type Syntax = super::Syntax;

    fn build_contract(&self, input: &Self::Input) -> Result<Self::Contract> {
        CBridgeContract::from_bindings(input, self.path.clone())
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        let header = template::Header::render(contract)?;
        FileLayout::single(self.path.clone()).assemble([Emitted::primary(header)])
    }
}

impl sealed::BridgeBackend for CBridge {}

impl HeaderInclude {
    /// Creates an include path relative to a generated C source file.
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
        if header_path.is_absolute() {
            return header_path.to_path_buf();
        }

        let Some(source_dir) = source_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        else {
            return header_path.to_path_buf();
        };

        match Self::relative_path(source_dir, header_path) {
            Some(path) if !path.as_os_str().is_empty() => path,
            _ => header_path
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| header_path.to_path_buf()),
        }
    }

    fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
        let from = Self::normal_components(from)?;
        let to = Self::normal_components(to)?;
        let shared = from
            .iter()
            .zip(&to)
            .take_while(|(left, right)| left == right)
            .count();
        Some(
            from.iter()
                .skip(shared)
                .map(|_| OsString::from(".."))
                .chain(to.into_iter().skip(shared))
                .collect(),
        )
    }

    fn normal_components(path: &Path) -> Option<Vec<OsString>> {
        path.components()
            .try_fold(Vec::new(), |mut parts, component| {
                match component {
                    Component::Prefix(prefix) => parts.push(prefix.as_os_str().to_owned()),
                    Component::RootDir => parts.push(component.as_os_str().to_owned()),
                    Component::CurDir => {}
                    Component::ParentDir => {
                        parts.pop()?;
                    }
                    Component::Normal(part) => parts.push(part.to_owned()),
                }
                Some(parts)
            })
    }
}

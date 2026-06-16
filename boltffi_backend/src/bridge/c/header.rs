use boltffi_binding::{Bindings, Native};

use crate::core::{
    Emitted, FileLayout, FilePath, GeneratedOutput, Result, bridge, contract::sealed,
};

use super::{contract::CBridgeContract, template};

/// C bridge backend for native bindings.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CBridge {
    path: FilePath,
}

impl CBridge {
    /// Creates a C header bridge.
    pub fn new(path: impl Into<std::path::PathBuf>) -> Result<Self> {
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

    fn build_contract(&self, input: &Self::Input) -> Result<Self::Contract> {
        CBridgeContract::from_bindings(input, self.path.clone())
    }

    fn render_bridge(
        &self,
        _input: &Self::Input,
        contract: &Self::Contract,
    ) -> Result<GeneratedOutput> {
        let header = template::Header::new(contract).render()?;
        FileLayout::single(self.path.clone()).assemble([Emitted::primary(header)])
    }
}

impl sealed::BridgeBackend for CBridge {}

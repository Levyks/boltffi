use std::path::{Path, PathBuf};

use boltffi_ast::PackageInfo;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ScanInput {
    root: PathBuf,
    package: PackageInfo,
}

impl ScanInput {
    pub fn new(root: impl Into<PathBuf>, package: PackageInfo) -> Self {
        Self {
            root: root.into(),
            package,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn package(&self) -> &PackageInfo {
        &self.package
    }
}

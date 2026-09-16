use std::fs;
use std::path::{Path, PathBuf};

use boltffi_ast::PackageInfo;
use boltffi_scan::{ActiveCfg, ScanInput, scan_package};

const ROOT_MANIFEST: &str = r#"
[package]
name = "root-package"
version = "0.1.0"
edition = "2024"

[features]
default = ["root-only"]
root-only = ["feature-dependency/enabled-api"]

[dependencies]
feature-dependency = { path = "dependency" }

[workspace]
members = ["dependency"]
resolver = "2"
"#;

const ROOT_SOURCE: &str = r#"
pub use feature_dependency::Enabled;

#[cfg(feature = "root-only")]
#[data]
pub struct RootFeature {
    pub value: u32,
}

#[export]
pub fn echo(value: Enabled) -> Enabled {
    value
}
"#;

const DEPENDENCY_MANIFEST: &str = r#"
[package]
name = "feature-dependency"
version = "0.1.0"
edition = "2024"

[features]
enabled-api = []
root-only = []
"#;

const DEPENDENCY_SOURCE: &str = r#"
#[cfg(feature = "enabled-api")]
#[data]
pub struct Enabled {
    pub value: u32,
}

#[data(impl)]
impl Enabled {
    #[cfg(feature = "enabled-api")]
    pub fn enabled_value(&self) -> u32 {
        self.value
    }

    #[cfg(feature = "root-only")]
    pub fn leaked_root_feature(&self) -> u32 {
        self.value
    }
}

#[cfg(not(feature = "enabled-api"))]
#[data]
pub struct Disabled {
    pub value: u32,
}

#[cfg(feature = "root-only")]
#[data]
pub struct LeakedRootFeature {
    pub value: u32,
}
"#;

struct PackageFixture {
    manifest_dir: PathBuf,
    source: PathBuf,
}

impl PackageFixture {
    fn create() -> Self {
        let manifest_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
            .join("dependency-features")
            .join(std::process::id().to_string());
        let dependency_dir = manifest_dir.join("dependency");
        let source_dir = manifest_dir.join("src");
        let dependency_source_dir = dependency_dir.join("src");
        [source_dir.as_path(), dependency_source_dir.as_path()]
            .into_iter()
            .try_for_each(fs::create_dir_all)
            .expect("fixture directories");
        Self::write(manifest_dir.join("Cargo.toml"), ROOT_MANIFEST);
        Self::write(source_dir.join("lib.rs"), ROOT_SOURCE);
        Self::write(dependency_dir.join("Cargo.toml"), DEPENDENCY_MANIFEST);
        Self::write(dependency_source_dir.join("lib.rs"), DEPENDENCY_SOURCE);
        Self {
            source: source_dir.join("lib.rs"),
            manifest_dir,
        }
    }

    fn write(path: impl AsRef<Path>, contents: &str) {
        fs::write(path, contents).expect("fixture file");
    }
}

#[test]
fn dependency_scan_uses_its_resolved_features() {
    let fixture = PackageFixture::create();
    let scan = scan_package(
        &ScanInput::new(
            &fixture.source,
            PackageInfo::new("root-package", Some("0.1.0".to_owned())),
        )
        .with_manifest_dir(&fixture.manifest_dir)
        .with_cfg(ActiveCfg::default().with_feature("root-only")),
    )
    .expect("package scan");
    let record_ids = scan
        .complete()
        .records
        .iter()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();

    assert!(record_ids.contains(&"feature_dependency::Enabled"));
    assert!(record_ids.contains(&"root_package::RootFeature"));
    assert!(!record_ids.contains(&"feature_dependency::Disabled"));
    assert!(!record_ids.contains(&"feature_dependency::LeakedRootFeature"));
    assert_eq!(scan.root().functions.len(), 1);

    let enabled = scan
        .complete()
        .records
        .iter()
        .find(|record| record.id.as_str() == "feature_dependency::Enabled")
        .expect("enabled dependency record");
    let method_names = enabled
        .methods
        .iter()
        .map(|method| method.name.spelling())
        .collect::<Vec<_>>();

    assert_eq!(method_names, ["enabled_value"]);
}

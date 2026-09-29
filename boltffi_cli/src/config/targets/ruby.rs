use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[targets.ruby]` configuration for the experimental Ruby target.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RubyConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Directory that receives the generated gem sources.
    #[serde(default = "default_ruby_output")]
    pub output: PathBuf,
    /// Gem name. Defaults to the Cargo package name.
    #[serde(default)]
    pub gem_name: Option<String>,
    /// Ruby module that holds the functions and records, such as `Demo` or
    /// `MyLib::Native`. Defaults to the package name in `UpperCamelCase`.
    #[serde(default)]
    pub module_name: Option<String>,
    /// Gem version. Defaults to the Cargo package version.
    #[serde(default)]
    pub version: Option<String>,
    /// Crate manifest, relative to the generated extension directory, that the
    /// generated `extconf.rb` builds when no prebuilt static library exists.
    #[serde(default)]
    pub cargo_manifest: Option<String>,
    /// Cargo arguments for every cargo invocation that builds this target.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cargo_args: Vec<String>,
}

impl Default for RubyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            output: default_ruby_output(),
            gem_name: None,
            module_name: None,
            version: None,
            cargo_manifest: None,
            cargo_args: Vec::new(),
        }
    }
}

fn default_ruby_output() -> PathBuf {
    PathBuf::from("dist/ruby")
}

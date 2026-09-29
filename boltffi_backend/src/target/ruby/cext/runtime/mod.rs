//! Handwritten C support code that ships with every generated extension.
//!
//! `boltffi_ruby.h` is not a template. Each extension includes it as written.

use crate::core::{FilePath, GeneratedFile, Result};

/// The runtime header that every generated extension includes.
const HEADER: &str = include_str!("../../../../../templates/target/ruby/runtime/boltffi_ruby.h");

/// Returns the runtime header for the extension sources in `directory`.
pub fn header(directory: &str) -> Result<GeneratedFile> {
    Ok(GeneratedFile::new(
        FilePath::new(format!("{directory}/boltffi_ruby.h"))?,
        HEADER,
    ))
}

//! The symbols a built library exports, to check generated bindings against.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use object::read::archive::ArchiveFile;
use object::{BinaryFormat, File, FileKind, NameOrOrdinal, Object, ObjectSymbol};
use thiserror::Error;

/// Symbols the bindings call that the built library does not export.
#[derive(Debug, Error)]
pub enum LibrarySymbolsError {
    /// The library could not be read.
    #[error("read built library `{path}`: {source}")]
    Read {
        /// Library path.
        path: PathBuf,
        /// Filesystem error.
        source: std::io::Error,
    },
    /// The library is not an object format the check understands.
    #[error("parse built library `{path}`: {source}")]
    Parse {
        /// Library path.
        path: PathBuf,
        /// Object parse error.
        source: object::Error,
    },
    /// Bindings name symbols the library lacks, so they would fail when loaded.
    #[error(
        "bindings call symbols that `{path}` does not export: {}; \
         the crates declaring them are not linked into the library; \
         name each such crate from the library crate, for example `use <crate> as _;`",
        missing.join(", ")
    )]
    Missing {
        /// Library path.
        path: PathBuf,
        /// Missing symbol names, sorted.
        missing: Vec<String>,
    },
    /// Bindings name symbols not found, and archive members that could hold them are unreadable.
    #[error(
        "bindings call symbols not found in `{path}`: {}; \
         its members {} are not object files the check can read, such as LLVM bitcode \
         from `-Clinker-plugin-lto`; build the library without bitcode members",
        missing.join(", "),
        unreadable.join(", ")
    )]
    Unreadable {
        /// Library path.
        path: PathBuf,
        /// Missing symbol names, sorted.
        missing: Vec<String>,
        /// Archive members that did not parse as object files.
        unreadable: Vec<String>,
    },
}

/// Checks that `library` exports every name in `symbols`.
pub fn require_exported<'name>(
    library: &Path,
    symbols: impl IntoIterator<Item = &'name str>,
) -> Result<(), LibrarySymbolsError> {
    let Exported {
        symbols: exported,
        unreadable,
    } = exported(library)?;
    let mut missing = symbols
        .into_iter()
        .filter(|symbol| !exported.contains(*symbol))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort();
    missing.dedup();
    let path = library.to_path_buf();
    if unreadable.is_empty() {
        return Err(LibrarySymbolsError::Missing { path, missing });
    }
    Err(LibrarySymbolsError::Unreadable {
        path,
        missing,
        unreadable,
    })
}

struct Exported {
    symbols: HashSet<String>,
    unreadable: Vec<String>,
}

impl Exported {
    fn readable(symbols: HashSet<String>) -> Self {
        Self {
            symbols,
            unreadable: Vec::new(),
        }
    }
}

fn exported(library: &Path) -> Result<Exported, LibrarySymbolsError> {
    let bytes = std::fs::read(library).map_err(|source| LibrarySymbolsError::Read {
        path: library.to_path_buf(),
        source,
    })?;
    let parse = |source| LibrarySymbolsError::Parse {
        path: library.to_path_buf(),
        source,
    };
    match FileKind::parse(bytes.as_slice()).map_err(parse)? {
        FileKind::Archive => {
            let archive = ArchiveFile::parse(bytes.as_slice()).map_err(parse)?;
            let mut exported = Exported::readable(HashSet::new());
            for member in archive.members() {
                let member = member.map_err(parse)?;
                let data = member.data(bytes.as_slice()).map_err(parse)?;
                match File::parse(data) {
                    Ok(file) => exported.symbols.extend(defined_globals(&file)),
                    Err(_) => exported
                        .unreadable
                        .push(String::from_utf8_lossy(member.name()).into_owned()),
                }
            }
            Ok(exported)
        }
        _ => {
            let file = File::parse(bytes.as_slice()).map_err(parse)?;
            if file.format() == BinaryFormat::Wasm {
                return Ok(Exported::readable(
                    defined_globals(&file).into_iter().collect(),
                ));
            }
            let exports = file.exports().map_err(parse)?;
            exports
                .map(|export| {
                    export.map_err(parse).map(|export| match export.name() {
                        NameOrOrdinal::Name(name) => std::str::from_utf8(name)
                            .ok()
                            .map(|name| unmangled(file.format(), name)),
                        NameOrOrdinal::Ordinal(_) => None,
                    })
                })
                .filter_map(Result::transpose)
                .collect::<Result<_, _>>()
                .map(Exported::readable)
        }
    }
}

fn defined_globals(file: &File<'_>) -> Vec<String> {
    file.symbols()
        .filter(|symbol| symbol.is_global() && !symbol.is_undefined())
        .filter_map(|symbol| {
            symbol
                .name()
                .ok()
                .map(|name| unmangled(file.format(), name))
        })
        .collect()
}

fn unmangled(format: BinaryFormat, name: &str) -> String {
    match format {
        BinaryFormat::MachO => name.strip_prefix('_').unwrap_or(name).to_owned(),
        _ => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut bytes = b"!<arch>\n".to_vec();
        for (name, data) in members {
            let header = format!(
                "{:<16}{:<12}{:<6}{:<6}{:<8}{:<10}`\n",
                format!("{name}/"),
                0,
                0,
                0,
                644,
                data.len()
            );
            bytes.extend_from_slice(header.as_bytes());
            bytes.extend_from_slice(data);
            if data.len() % 2 == 1 {
                bytes.push(b'\n');
            }
        }
        bytes
    }

    #[test]
    fn missing_symbols_name_the_archive_members_that_could_not_be_read() {
        let directory =
            std::env::temp_dir().join(format!("boltffi-library-symbols-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir is created");
        let library = directory.join("libbitcode.a");
        std::fs::write(
            &library,
            archive(&[("lib.bc", b"BC\xC0\xDE\x35\x14\x00\x00")]),
        )
        .expect("archive is written");

        let error = require_exported(&library, ["boltffi_function_demo_answer"])
            .expect_err("a symbol only a bitcode member could define is not verified");

        assert!(matches!(
            &error,
            LibrarySymbolsError::Unreadable { missing, unreadable, .. }
                if missing == &["boltffi_function_demo_answer"] && unreadable == &["lib.bc"]
        ));
        assert!(
            error.to_string().contains("linker-plugin-lto"),
            "the error points at bitcode instead of an unlinked crate: {error}"
        );
        std::fs::remove_dir_all(&directory).expect("temp dir is removed");
    }
}

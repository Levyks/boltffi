use boltffi_binding::{CanonicalName, NamePart};

use crate::core::{Error, Result};

use super::syntax::Identifier;

pub struct Name<'source> {
    source: &'source CanonicalName,
}

/// A Python package module name accepted by generated Python bindings.
///
/// The value is guaranteed to be a Python identifier and not a Python keyword.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageModule {
    name: String,
}

impl<'source> Name<'source> {
    pub fn new(source: &'source CanonicalName) -> Self {
        Self { source }
    }

    pub fn function(&self) -> Result<Identifier> {
        let name = self
            .source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_");
        Identifier::escape(name)
    }

    pub fn function_text(&self) -> Result<String> {
        self.function().map(|identifier| identifier.to_string())
    }

    pub fn class(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .map(capitalized)
            .collect()
    }

    pub fn enum_member(&self) -> String {
        self.source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .map(str::to_ascii_uppercase)
            .collect::<Vec<_>>()
            .join("_")
    }
}

impl PackageModule {
    /// Parses a configured Python package module name.
    ///
    /// Returns an error when the name is empty, is not a Python identifier, or is a Python keyword.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if Identifier::parse(name.clone()).is_ok() {
            Ok(Self { name })
        } else {
            Err(Error::InvalidPythonPackageModule { name })
        }
    }

    /// Creates a package module name from a canonical BoltFFI package name.
    pub fn from_canonical(name: &CanonicalName) -> Result<Self> {
        Ok(Self {
            name: Name::new(name).function_text()?,
        })
    }

    /// Returns the Python module name.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

pub fn valid_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first_character) = characters.next() else {
        return false;
    };
    (first_character == '_' || first_character.is_alphabetic())
        && characters.all(|character| character == '_' || character.is_alphanumeric())
}

fn capitalized(part: &str) -> String {
    let mut characters = part.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

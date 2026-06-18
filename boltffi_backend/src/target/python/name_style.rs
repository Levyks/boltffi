use boltffi_binding::{CanonicalName, NamePart};

use crate::core::{Error, Result};

pub struct Name<'source> {
    source: &'source CanonicalName,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
/// A Python package module name accepted by generated Python bindings.
///
/// The value is guaranteed to be a Python identifier and not a Python keyword.
pub struct PackageModule {
    name: String,
}

impl<'source> Name<'source> {
    pub fn new(source: &'source CanonicalName) -> Self {
        Self { source }
    }

    pub fn function(&self) -> String {
        let name = self
            .source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_");
        match keyword(&name) {
            true => format!("{name}_"),
            false => name,
        }
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
        if Self::identifier(&name) && !keyword(&name) {
            Ok(Self { name })
        } else {
            Err(Error::InvalidPythonPackageModule { name })
        }
    }

    /// Creates a package module name from a canonical BoltFFI package name.
    pub fn from_canonical(name: &CanonicalName) -> Self {
        Self {
            name: Name::new(name).function(),
        }
    }

    /// Returns the Python module name.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    fn identifier(name: &str) -> bool {
        let mut characters = name.chars();
        let Some(first_character) = characters.next() else {
            return false;
        };
        (first_character == '_' || first_character.is_alphabetic())
            && characters.all(|character| character == '_' || character.is_alphanumeric())
    }
}

fn capitalized(part: &str) -> String {
    let mut characters = part.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

fn keyword(name: &str) -> bool {
    matches!(
        name,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "case"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "match"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "type"
            | "while"
            | "with"
            | "yield"
    )
}

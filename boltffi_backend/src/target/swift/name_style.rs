use std::{fmt, path::PathBuf};

use boltffi_binding::{CanonicalName, NamePart};

use crate::{
    core::{Error, Result},
    target::swift::syntax::{Identifier, TypeName},
};

/// A Swift module name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SwiftModule {
    name: String,
}

/// A Swift source file name.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SwiftFile {
    name: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Name {
    parts: Vec<NamePart>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GeneratedLocal {
    ReturnBuffer,
    ReturnHandle,
    ErrorBuffer,
    WireReader,
    ErrorReader,
    FutureHandle,
    FutureStatus,
    ClosureStatus,
    ClosureInvoke,
    StreamSubscription,
    StreamBatch,
    StreamBatchCount,
}

impl SwiftModule {
    /// Parses a Swift module name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if SwiftFile::valid(&name) {
            Ok(Self { name })
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier: name })
        }
    }

    /// Returns the module name text.
    pub fn as_str(&self) -> &str {
        &self.name
    }
}

impl fmt::Display for SwiftModule {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl SwiftFile {
    /// Parses a Swift source file name.
    pub fn parse(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if Self::valid(&name) {
            Ok(Self { name })
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier: name })
        }
    }

    /// Creates the default source file for a module.
    pub fn from_module(module: &SwiftModule) -> Self {
        Self {
            name: module.as_str().to_owned(),
        }
    }

    /// Returns the generated source path.
    pub fn path(&self) -> PathBuf {
        PathBuf::from(format!("{}.swift", self.name))
    }

    fn valid(name: &str) -> bool {
        let mut characters = name.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl fmt::Display for SwiftFile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

impl Name {
    pub fn new(name: &CanonicalName) -> Self {
        Self {
            parts: name.parts().to_vec(),
        }
    }

    pub fn function(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn parameter(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn field(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn variant(&self) -> Result<Identifier> {
        Identifier::escape(self.lower_camel())
    }

    pub fn generated(&self, suffix: &str) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi{}{}",
            self.upper_camel(),
            Self::upper_camel_from_snake(suffix)
        ))
    }

    pub fn type_name(&self) -> TypeName {
        TypeName::new(
            self.parts
                .iter()
                .map(|part| Self::capitalized(part.as_str()))
                .collect::<String>(),
        )
    }

    fn lower_camel(&self) -> String {
        self.parts
            .iter()
            .enumerate()
            .map(|(index, part)| match index {
                0 => part.as_str().to_owned(),
                _ => Self::capitalized(part.as_str()),
            })
            .collect()
    }

    fn upper_camel(&self) -> String {
        self.parts
            .iter()
            .map(|part| Self::capitalized(part.as_str()))
            .collect()
    }

    fn upper_camel_from_snake(name: &str) -> String {
        name.split('_').map(Self::capitalized).collect()
    }

    fn capitalized(part: &str) -> String {
        let mut characters = part.chars();
        characters.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(characters).collect()
        })
    }
}

impl GeneratedLocal {
    pub fn identifier(self) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi{}",
            Name::upper_camel_from_snake(self.role())
        ))
    }

    pub fn suffixed(self, suffix: &str) -> Result<Identifier> {
        Identifier::parse(format!(
            "boltffi{}{}",
            Name::upper_camel_from_snake(self.role()),
            Name::upper_camel_from_snake(suffix)
        ))
    }

    fn role(self) -> &'static str {
        match self {
            Self::ReturnBuffer => "result",
            Self::ReturnHandle => "handle",
            Self::ErrorBuffer => "error",
            Self::WireReader => "reader",
            Self::ErrorReader => "error_reader",
            Self::FutureHandle => "future",
            Self::FutureStatus => "status",
            Self::ClosureStatus => "closure_status",
            Self::ClosureInvoke => "closure_invoke",
            Self::StreamSubscription => "subscription",
            Self::StreamBatch => "stream_batch",
            Self::StreamBatchCount => "stream_count",
        }
    }
}

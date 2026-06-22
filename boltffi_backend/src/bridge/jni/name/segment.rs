use boltffi_binding::{CanonicalName, ClosureSignature};

use crate::{
    bridge::jni::name::JniSymbolName,
    core::{Error, Result},
};

/// One JVM package or class-name segment.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JvmNameSegment {
    name: String,
}

impl JvmNameSegment {
    /// Returns the JVM source spelling.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    pub(in crate::bridge::jni::name) fn package(name: &str) -> Result<Self> {
        Self::parse(name.to_owned(), |name| Error::InvalidJvmPackageName {
            name,
        })
    }

    pub(in crate::bridge::jni::name) fn class(name: impl Into<String>) -> Result<Self> {
        Self::parse(name.into(), |name| Error::InvalidJvmClassName { name })
    }

    pub(in crate::bridge::jni::name) fn callback_class(callback: &CanonicalName) -> Result<Self> {
        Self::class(format!("{}Callbacks", Self::canonical_class(callback)))
    }

    pub(in crate::bridge::jni::name) fn closure_class(
        signature: &ClosureSignature,
    ) -> Result<Self> {
        Self::class(format!("Closure{}Callbacks", signature.as_str()))
    }

    pub(in crate::bridge::jni::name) fn jni_escape(&self) -> String {
        JniSymbolName::escape_part(&self.name)
    }

    fn canonical_class(name: &CanonicalName) -> String {
        name.parts()
            .iter()
            .map(|part| Self::capitalized(part.as_str()))
            .collect()
    }

    fn capitalized(segment: &str) -> String {
        let mut characters = segment.chars();
        characters.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(characters).collect()
        })
    }

    fn parse(name: String, error: impl FnOnce(String) -> Error) -> Result<Self> {
        if Self::valid(&name) {
            Ok(Self { name })
        } else {
            Err(error(name))
        }
    }

    fn valid(name: &str) -> bool {
        let mut characters = name.chars();
        characters.next().is_some_and(|character| {
            character == '_' || character == '$' || character.is_ascii_alphabetic()
        }) && characters.all(|character| {
            character == '_' || character == '$' || character.is_ascii_alphanumeric()
        })
    }
}

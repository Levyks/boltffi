use std::fmt;

use boltffi_binding::CanonicalName;

use crate::core::{Error, Result};

use crate::bridge::c::Identifier;

/// Fully qualified JVM class that owns generated native methods.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JvmClassPath {
    package: Vec<JvmNameSegment>,
    class: JvmNameSegment,
}

/// One JVM package or class-name segment.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JvmNameSegment {
    name: String,
}

/// C symbol name exported through JNI's `Java_*` naming convention.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JniSymbolName {
    identifier: Identifier,
}

impl JvmClassPath {
    /// Creates a JVM class path from a package name and class name.
    pub fn new(package: impl Into<String>, class: impl Into<String>) -> Result<Self> {
        let package = package.into();
        let package = match package.is_empty() {
            true => Vec::new(),
            false => package
                .split('.')
                .map(JvmNameSegment::package)
                .collect::<Result<Vec<_>>>()?,
        };
        Ok(Self {
            package,
            class: JvmNameSegment::class(class)?,
        })
    }

    /// Returns the Java source spelling of this class path.
    pub fn as_java_path(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(JvmNameSegment::as_str)
            .collect::<Vec<_>>()
            .join(".")
    }

    /// Returns the slash-separated class name used by JNI class lookup.
    pub fn as_jni_class_name(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(JvmNameSegment::as_str)
            .collect::<Vec<_>>()
            .join("/")
    }

    /// Creates the generated callback bridge class in the same JVM package.
    pub fn callback_class(&self, callback: &CanonicalName) -> Result<Self> {
        Ok(Self {
            package: self.package.clone(),
            class: JvmNameSegment::callback_class(callback)?,
        })
    }

    /// Returns the class path as the prefix used by a JNI exported symbol.
    pub fn jni_prefix(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(|segment| segment.jni_escape())
            .collect::<Vec<_>>()
            .join("_")
    }
}

impl fmt::Display for JvmClassPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_java_path())
    }
}

impl JniSymbolName {
    /// Creates a JNI native-method symbol for a JVM class and method name.
    pub fn native_method(class: &JvmClassPath, method: &str) -> Result<Self> {
        Ok(Self {
            identifier: Identifier::parse(format!(
                "Java_{}_{}",
                class.jni_prefix(),
                Self::escape_part(method)
            ))?,
        })
    }

    /// Returns the generated C identifier.
    pub fn as_identifier(&self) -> &Identifier {
        &self.identifier
    }

    fn escape_part(part: &str) -> String {
        part.chars().fold(String::new(), |mut escaped, character| {
            match character {
                '_' => escaped.push_str("_1"),
                ';' => escaped.push_str("_2"),
                '[' => escaped.push_str("_3"),
                character => escaped.push(character),
            }
            escaped
        })
    }
}

impl fmt::Display for JniSymbolName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identifier.fmt(formatter)
    }
}

impl JvmNameSegment {
    /// Returns the JVM source spelling.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    fn package(name: &str) -> Result<Self> {
        Self::parse(name.to_owned(), |name| Error::InvalidJvmPackageName {
            name,
        })
    }

    fn class(name: impl Into<String>) -> Result<Self> {
        Self::parse(name.into(), |name| Error::InvalidJvmClassName { name })
    }

    fn callback_class(callback: &CanonicalName) -> Result<Self> {
        Self::class(format!("{}Callbacks", Self::canonical_class(callback)))
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

    fn jni_escape(&self) -> String {
        JniSymbolName::escape_part(&self.name)
    }
}

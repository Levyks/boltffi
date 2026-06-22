use std::fmt;

use crate::{
    bridge::{c::Identifier, jni::name::JvmClassPath},
    core::Result,
};

/// C symbol name exported through JNI's `Java_*` naming convention.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JniSymbolName {
    identifier: Identifier,
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

    pub(in crate::bridge::jni::name) fn escape_part(part: &str) -> String {
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

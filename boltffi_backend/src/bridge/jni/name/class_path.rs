use std::fmt;

use boltffi_binding::{CanonicalName, ClosureSignature};

use crate::{bridge::jni::name::JvmNameSegment, core::Result};

/// Fully qualified JVM class that owns generated native methods.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub struct JvmClassPath {
    package: Vec<JvmNameSegment>,
    class: JvmNameSegment,
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

    /// Creates the generated closure bridge class in the same JVM package.
    pub fn closure_class(&self, signature: &ClosureSignature) -> Result<Self> {
        Ok(Self {
            package: self.package.clone(),
            class: JvmNameSegment::closure_class(signature)?,
        })
    }

    /// Returns the class path as the prefix used by a JNI exported symbol.
    pub fn jni_prefix(&self) -> String {
        self.package
            .iter()
            .chain(std::iter::once(&self.class))
            .map(JvmNameSegment::jni_escape)
            .collect::<Vec<_>>()
            .join("_")
    }
}

impl fmt::Display for JvmClassPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.as_java_path())
    }
}

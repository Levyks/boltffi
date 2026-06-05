use serde::{Deserialize, Serialize};

use crate::{
    ClassId, DeprecationInfo, DocComment, MethodDef, Source, SourceName, SourceSpan, UserAttr,
};

/// A class-style Rust object exported through BoltFFI.
///
/// A class groups associated functions and methods around an owned
/// Rust value. Associated functions that return `Self` stay as methods here;
/// binding layers can present them as creation entry points later.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    /// Stable class identity derived from the canonical Rust path.
    pub id: ClassId,
    /// Source class name.
    pub name: SourceName,
    /// Methods attached to the class.
    pub methods: Vec<MethodDef>,
    /// User attributes preserved from the class declaration.
    pub user_attrs: Vec<UserAttr>,
    /// Documentation attached to the class.
    pub doc: Option<DocComment>,
    /// Deprecation metadata attached to the class.
    pub deprecated: Option<DeprecationInfo>,
    /// Visibility and source location for diagnostics.
    pub source: Source,
    /// Span available during macro expansion.
    #[serde(default, skip_serializing, skip_deserializing)]
    pub source_span: Option<SourceSpan>,
}

impl ClassDef {
    /// Builds an empty class definition.
    ///
    /// The `id` parameter is the stable class ID. The `name` parameter is the
    /// canonical source name.
    ///
    /// Returns a class with no methods, attributes, or docs.
    pub fn new(id: ClassId, name: impl Into<SourceName>) -> Self {
        Self {
            id,
            name: name.into(),
            methods: Vec::new(),
            user_attrs: Vec::new(),
            doc: None,
            deprecated: None,
            source: Source::exported(),
            source_span: None,
        }
    }
}

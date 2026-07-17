//! Minimal typed Dart syntax fragments (identical language to target::dart;
//! duplicated rather than shared because target::dart's module is private).

use std::fmt;

use crate::core::{LanguageSyntax, syntax::sealed};

/// A validated or renderer-owned Dart source fragment.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fragment(String);

impl Fragment {
    /// Creates a renderer-owned Dart fragment.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the underlying source.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Fragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Fragment {}

/// Dart syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "abstract",
        "as",
        "assert",
        "async",
        "await",
        "break",
        "case",
        "catch",
        "class",
        "const",
        "continue",
        "covariant",
        "default",
        "deferred",
        "do",
        "dynamic",
        "else",
        "enum",
        "export",
        "extends",
        "extension",
        "external",
        "factory",
        "false",
        "final",
        "finally",
        "for",
        "Function",
        "get",
        "hide",
        "if",
        "implements",
        "import",
        "in",
        "interface",
        "is",
        "late",
        "library",
        "mixin",
        "new",
        "null",
        "on",
        "operator",
        "part",
        "required",
        "rethrow",
        "return",
        "sealed",
        "set",
        "show",
        "static",
        "super",
        "switch",
        "sync",
        "this",
        "throw",
        "true",
        "try",
        "typedef",
        "var",
        "void",
        "when",
        "while",
        "with",
        "yield",
    ];

    type Identifier = Fragment;
    type Type = Fragment;
    type Expr = Fragment;
    type Stmt = Fragment;
    type Literal = Fragment;
    type Arguments = Fragment;
}

impl sealed::LanguageSyntax for Syntax {}

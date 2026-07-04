use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

/// Swift syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A valid Swift identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier(String);

/// Swift type syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeName(String);

/// Swift expression syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

/// Swift statement syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

/// Swift literal syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

/// Swift argument-list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "associatedtype",
        "borrowing",
        "class",
        "consuming",
        "deinit",
        "enum",
        "extension",
        "fileprivate",
        "func",
        "import",
        "init",
        "inout",
        "internal",
        "let",
        "nonisolated",
        "open",
        "operator",
        "precedencegroup",
        "private",
        "protocol",
        "public",
        "rethrows",
        "static",
        "struct",
        "subscript",
        "typealias",
        "var",
        "break",
        "case",
        "catch",
        "continue",
        "default",
        "defer",
        "do",
        "else",
        "fallthrough",
        "for",
        "guard",
        "if",
        "in",
        "repeat",
        "return",
        "switch",
        "throw",
        "where",
        "while",
        "Any",
        "as",
        "await",
        "false",
        "is",
        "nil",
        "self",
        "Self",
        "super",
        "throws",
        "true",
        "try",
        "_",
    ];

    type Identifier = Identifier;
    type Type = TypeName;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl sealed::SyntaxFragment for Identifier {}

impl fmt::Display for Identifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Identifier {
    pub fn parse(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Self::valid(&identifier) && !Syntax::keyword(&identifier) {
            Ok(Self(identifier))
        } else {
            Err(Error::InvalidSwiftIdentifier { identifier })
        }
    }

    pub fn escape(identifier: impl Into<String>) -> Result<Self> {
        let identifier = identifier.into();
        if Syntax::keyword(&identifier) {
            Ok(Self(format!("`{identifier}`")))
        } else {
            Self::parse(identifier)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn valid(identifier: &str) -> bool {
        let mut characters = identifier.chars();
        characters
            .next()
            .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
            && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
    }
}

impl sealed::SyntaxFragment for TypeName {}

impl fmt::Display for TypeName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TypeName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn void() -> Self {
        Self::new("Void")
    }

    pub fn bool() -> Self {
        Self::new("Bool")
    }

    pub fn string() -> Self {
        Self::new("String")
    }

    pub fn data() -> Self {
        Self::new("Data")
    }

    pub fn int8() -> Self {
        Self::new("Int8")
    }

    pub fn uint8() -> Self {
        Self::new("UInt8")
    }

    pub fn int16() -> Self {
        Self::new("Int16")
    }

    pub fn uint16() -> Self {
        Self::new("UInt16")
    }

    pub fn int32() -> Self {
        Self::new("Int32")
    }

    pub fn uint32() -> Self {
        Self::new("UInt32")
    }

    pub fn int64() -> Self {
        Self::new("Int64")
    }

    pub fn uint64() -> Self {
        Self::new("UInt64")
    }

    pub fn int() -> Self {
        Self::new("Int")
    }

    pub fn uint() -> Self {
        Self::new("UInt")
    }

    pub fn float() -> Self {
        Self::new("Float")
    }

    pub fn double() -> Self {
        Self::new("Double")
    }

    pub fn array(element: Self) -> Self {
        Self::new(format!("[{element}]"))
    }

    pub fn optional(self) -> Self {
        Self::new(format!("{self}?"))
    }

    pub fn result(ok: Self, err: Self) -> Self {
        Self::new(format!("Result<{ok}, {err}>"))
    }

    pub fn dictionary(key: Self, value: Self) -> Self {
        Self::new(format!("[{key}: {value}]"))
    }
}

impl sealed::SyntaxFragment for Expression {}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Expression {
    pub fn new(expression: impl Into<String>) -> Self {
        Self(expression.into())
    }

    pub fn member(base: impl fmt::Display, member: impl fmt::Display) -> Self {
        Self::new(format!("{base}.{member}"))
    }

    pub fn call(callee: impl fmt::Display, arguments: ArgumentList) -> Self {
        Self::new(format!("{callee}({arguments})"))
    }

    pub fn labeled(label: impl fmt::Display, value: impl fmt::Display) -> Self {
        Self::new(format!("{label}: {value}"))
    }
}

impl sealed::SyntaxFragment for Statement {}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Statement {
    pub fn new(statement: impl Into<String>) -> Self {
        Self(statement.into())
    }
}

impl sealed::SyntaxFragment for Literal {}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Literal {
    pub fn new(literal: impl Into<String>) -> Self {
        Self(literal.into())
    }
}

impl sealed::SyntaxFragment for ArgumentList {}

impl fmt::Display for ArgumentList {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(
            &self
                .0
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}

impl FromIterator<Expression> for ArgumentList {
    fn from_iter<T: IntoIterator<Item = Expression>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

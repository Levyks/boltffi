use std::fmt;

use crate::core::{Error, LanguageSyntax, Result, syntax::sealed};

/// Kotlin syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// A valid Kotlin identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Identifier(String);

/// Kotlin type syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TypeName(String);

/// Kotlin expression syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Expression(String);

/// Kotlin statement syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Statement(String);

/// Kotlin literal syntax.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Literal(String);

/// Kotlin argument-list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct ArgumentList(Vec<Expression>);

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "as",
        "break",
        "class",
        "continue",
        "do",
        "else",
        "false",
        "for",
        "fun",
        "if",
        "in",
        "interface",
        "is",
        "null",
        "object",
        "package",
        "return",
        "super",
        "this",
        "throw",
        "true",
        "try",
        "typealias",
        "typeof",
        "val",
        "var",
        "when",
        "while",
        "by",
        "catch",
        "constructor",
        "delegate",
        "dynamic",
        "field",
        "file",
        "finally",
        "get",
        "import",
        "init",
        "param",
        "property",
        "receiver",
        "set",
        "setparam",
        "value",
        "where",
        "actual",
        "abstract",
        "annotation",
        "companion",
        "const",
        "crossinline",
        "data",
        "enum",
        "expect",
        "external",
        "final",
        "infix",
        "inline",
        "inner",
        "internal",
        "lateinit",
        "noinline",
        "open",
        "operator",
        "out",
        "override",
        "private",
        "protected",
        "public",
        "reified",
        "sealed",
        "suspend",
        "tailrec",
        "vararg",
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
            Err(Error::InvalidKotlinIdentifier { identifier })
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

    pub fn unit() -> Self {
        Self::new("Unit")
    }

    pub fn boolean() -> Self {
        Self::new("Boolean")
    }

    pub fn byte() -> Self {
        Self::new("Byte")
    }

    pub fn ubyte() -> Self {
        Self::new("UByte")
    }

    pub fn short() -> Self {
        Self::new("Short")
    }

    pub fn ushort() -> Self {
        Self::new("UShort")
    }

    pub fn int() -> Self {
        Self::new("Int")
    }

    pub fn uint() -> Self {
        Self::new("UInt")
    }

    pub fn long() -> Self {
        Self::new("Long")
    }

    pub fn ulong() -> Self {
        Self::new("ULong")
    }

    pub fn float() -> Self {
        Self::new("Float")
    }

    pub fn double() -> Self {
        Self::new("Double")
    }

    pub fn byte_array(nullable: bool) -> Self {
        Self::new(match nullable {
            true => "ByteArray?",
            false => "ByteArray",
        })
    }
}

impl sealed::SyntaxFragment for Expression {}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Expression {
    pub fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub fn call(receiver: impl fmt::Display, method: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{receiver}.{method}({arguments})"))
    }

    pub fn convert(self, method: Identifier) -> Self {
        Self(format!("{self}.{method}()"))
    }
}

impl sealed::SyntaxFragment for Statement {}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Statement {
    pub fn return_value(value: Expression) -> Self {
        Self(format!("return {value}"))
    }

    pub fn expression(value: Expression) -> Self {
        Self(value.to_string())
    }
}

impl sealed::SyntaxFragment for Literal {}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
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

impl ArgumentList {
    fn from_expressions(expressions: impl IntoIterator<Item = Expression>) -> Self {
        Self(expressions.into_iter().collect())
    }
}

impl FromIterator<Expression> for ArgumentList {
    fn from_iter<T: IntoIterator<Item = Expression>>(expressions: T) -> Self {
        Self::from_expressions(expressions)
    }
}

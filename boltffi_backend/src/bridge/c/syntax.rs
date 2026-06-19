use std::fmt;

use crate::core::{LanguageSyntax, Result, syntax::sealed};

use super::{
    contract::{Function, Parameter, Type},
    identifier::Identifier,
};

/// C syntax fragment family.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Syntax;

/// C type syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeFragment(String);

/// C expression syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Expression(String);

/// C statement syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Statement(String);

/// C literal syntax.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Literal(String);

/// C argument list syntax.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArgumentList(Vec<Expression>);

pub struct TypeSyntax<'ty> {
    ty: &'ty Type,
}

pub struct FunctionSyntax<'function> {
    function: &'function Function,
}

struct ParameterSyntax<'parameter> {
    parameter: &'parameter Parameter,
}

impl LanguageSyntax for Syntax {
    const KEYWORDS: &'static [&'static str] = &[
        "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
        "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
        "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
        "union", "unsigned", "void", "volatile", "while",
    ];

    type Identifier = Identifier;
    type Type = TypeFragment;
    type Expr = Expression;
    type Stmt = Statement;
    type Literal = Literal;
    type Arguments = ArgumentList;
}

impl sealed::LanguageSyntax for Syntax {}

impl sealed::SyntaxFragment for Identifier {}

impl fmt::Display for TypeFragment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for TypeFragment {}

impl TypeFragment {
    /// Creates C type syntax.
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Expression {}

impl Expression {
    pub(crate) fn identifier(identifier: Identifier) -> Self {
        Self(identifier.to_string())
    }

    pub(crate) fn literal(literal: Literal) -> Self {
        Self(literal.to_string())
    }

    pub(crate) fn call(function: Identifier, arguments: ArgumentList) -> Self {
        Self(format!("{function}({arguments})"))
    }

    pub(crate) fn address_of(expression: Self) -> Self {
        Self(format!("&{expression}"))
    }

    pub(crate) fn cast(ty: TypeFragment, expression: Self) -> Self {
        Self(format!("({ty}){expression}"))
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Statement {}

impl Statement {
    /// Creates C statement syntax.
    pub fn new(fragment: impl Into<String>) -> Self {
        Self(fragment.into())
    }
}

impl fmt::Display for Literal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl sealed::SyntaxFragment for Literal {}

impl Literal {
    pub(crate) fn integer_zero() -> Self {
        Self("0".to_owned())
    }

    pub(crate) fn bool_false() -> Self {
        Self("false".to_owned())
    }

    pub(crate) fn f32_zero() -> Self {
        Self("0.0f".to_owned())
    }

    pub(crate) fn f64_zero() -> Self {
        Self("0.0".to_owned())
    }

    pub(crate) fn compound_zero() -> Self {
        Self("{0}".to_owned())
    }

    pub(crate) fn string(value: &str) -> Self {
        Self(format!("{value:?}"))
    }
}

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

impl sealed::SyntaxFragment for ArgumentList {}

impl ArgumentList {
    pub(crate) fn from_iter(arguments: impl IntoIterator<Item = Expression>) -> Self {
        Self(arguments.into_iter().collect())
    }
}

impl<'ty> TypeSyntax<'ty> {
    pub fn new(ty: &'ty Type) -> Self {
        Self { ty }
    }

    pub fn anonymous(&self) -> Result<TypeFragment> {
        Ok(TypeFragment::new(match self.ty {
            Type::Void => "void".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Int8 => "int8_t".to_owned(),
            Type::Uint8 => "uint8_t".to_owned(),
            Type::Int16 => "int16_t".to_owned(),
            Type::Uint16 => "uint16_t".to_owned(),
            Type::Int32 => "int32_t".to_owned(),
            Type::Uint32 => "uint32_t".to_owned(),
            Type::Int64 => "int64_t".to_owned(),
            Type::Uint64 => "uint64_t".to_owned(),
            Type::Float32 => "float".to_owned(),
            Type::Float64 => "double".to_owned(),
            Type::SignedPointerWidth => "intptr_t".to_owned(),
            Type::PointerWidth => "uintptr_t".to_owned(),
            Type::Status => "FfiStatus".to_owned(),
            Type::Buffer => "FfiBuf_u8".to_owned(),
            Type::String => "FfiString".to_owned(),
            Type::Span => "FfiSpan".to_owned(),
            Type::FutureHandle => "RustFutureHandle".to_owned(),
            Type::StreamPollResult => "StreamPollResult".to_owned(),
            Type::WaitResult => "WaitResult".to_owned(),
            Type::CallbackHandle => "BoltFFICallbackHandle".to_owned(),
            Type::Named(name) => name.to_string(),
            Type::ConstPointer(inner) => format!("const {} *", Self::new(inner).anonymous()?),
            Type::MutPointer(inner) => format!("{} *", Self::new(inner).anonymous()?),
            Type::FunctionPointer { returns, params } => {
                Self::function_pointer_declaration("", returns, params.iter())?
                    .to_string()
                    .trim()
                    .to_owned()
            }
        }))
    }

    pub fn declaration(&self, name: &str) -> Result<Statement> {
        let name = Identifier::escape(name)?;
        Ok(Statement::new(match self.ty {
            Type::FunctionPointer { returns, params } => {
                Self::function_pointer_declaration(name.as_str(), returns, params.iter())?
                    .to_string()
            }
            Type::ConstPointer(inner) => {
                format!("const {} *{}", Self::new(inner).anonymous()?, name)
            }
            Type::MutPointer(inner) => format!("{} *{}", Self::new(inner).anonymous()?, name),
            _ => format!("{} {}", self.anonymous()?, name),
        }))
    }

    pub fn function(&self, name: &str, params: &str) -> Result<Statement> {
        Ok(Statement::new(format!(
            "{} {name}({params})",
            self.anonymous()?
        )))
    }
}

impl TypeSyntax<'_> {
    pub fn function_pointer_declaration<'params>(
        name: &str,
        returns: &Type,
        params: impl IntoIterator<Item = &'params Type>,
    ) -> Result<Statement> {
        let params = params
            .into_iter()
            .map(|ty| TypeSyntax { ty }.anonymous())
            .collect::<Result<Vec<_>>>()?;
        let params = match params.is_empty() {
            true => "void".to_owned(),
            false => params
                .into_iter()
                .map(|param| param.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        };
        Ok(Statement::new(format!(
            "{} (*{name})({params})",
            TypeSyntax { ty: returns }.anonymous()?
        )))
    }
}

impl<'function> FunctionSyntax<'function> {
    pub fn new(function: &'function Function) -> Self {
        Self { function }
    }

    pub fn declaration(&self) -> Result<Statement> {
        let name = Identifier::parse(self.function.name())?;
        TypeSyntax::new(self.function.returns()).function(name.as_str(), &self.named_params()?)
    }

    pub fn pointer_typedef(&self, name: &str) -> Result<Statement> {
        let name = Identifier::parse(name)?;
        Ok(Statement::new(format!(
            "typedef {}",
            TypeSyntax::function_pointer_declaration(
                name.as_str(),
                self.function.returns(),
                self.function.params().iter().map(Parameter::ty)
            )?
        )))
    }

    fn named_params(&self) -> Result<String> {
        match self.function.params().is_empty() {
            true => Ok("void".to_owned()),
            false => self
                .function
                .params()
                .iter()
                .map(ParameterSyntax::new)
                .map(|parameter| parameter.declaration())
                .collect::<Result<Vec<_>>>()
                .map(|params| {
                    params
                        .into_iter()
                        .map(|param| param.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                }),
        }
    }
}

impl<'parameter> ParameterSyntax<'parameter> {
    fn new(parameter: &'parameter Parameter) -> Self {
        Self { parameter }
    }

    fn declaration(&self) -> Result<Statement> {
        TypeSyntax::new(self.parameter.ty()).declaration(self.parameter.name())
    }
}

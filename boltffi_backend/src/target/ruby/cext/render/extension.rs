//! The extension source file: forward declarations, definitions, and `Init_`.

use askama::Template;
use boltffi_binding::Native;

use crate::{
    bridge::c::{Identifier, Literal},
    core::{AuxChunk, Error, FilePath, GeneratedFile, RenderedDeclaration, Result},
    target::ruby::syntax::ConstantPath,
};

#[derive(Template)]
#[template(path = "target/ruby/extension.c", escape = "none")]
struct ExtensionTemplate {
    init: Identifier,
    outer: Literal,
    nested: Vec<Literal>,
    forward_declarations: Vec<String>,
    definitions: Vec<String>,
}

/// The directory that holds the extension sources, such as `ext/my_lib`.
pub fn directory(stem: &str) -> String {
    format!("ext/{stem}")
}

/// Renders the extension source file, `<stem>.c`.
pub fn render(
    stem: &str,
    module: &ConstantPath,
    declarations: Vec<RenderedDeclaration<'_, Native>>,
) -> Result<GeneratedFile> {
    Ok(GeneratedFile::new(
        FilePath::new(format!("{}/{stem}.c", directory(stem)))?,
        source(stem, module, declarations)?,
    ))
}

/// Renders the C source of the extension.
fn source(
    stem: &str,
    module: &ConstantPath,
    declarations: Vec<RenderedDeclaration<'_, Native>>,
) -> Result<String> {
    let (outer, nested) = module
        .segments()
        .split_first()
        .ok_or(Error::InvalidRubyIdentifier {
            identifier: module.to_string(),
        })?;
    let mut forward_declarations = Vec::new();
    let mut definitions = Vec::new();
    for rendered in declarations {
        let (primary, aux, _) = rendered.into_parts().1.into_parts();
        forward_declarations.extend(aux.into_iter().filter_map(|chunk| match chunk {
            AuxChunk::ForwardDecl(text) => Some(text.into_string()),
            _ => None,
        }));
        if !primary.is_empty() {
            definitions.push(primary.into_string());
        }
    }
    let source = ExtensionTemplate {
        init: Identifier::parse(format!("Init_{stem}"))?,
        outer: Literal::string(outer.as_str()),
        nested: nested
            .iter()
            .map(|constant| Literal::string(constant.as_str()))
            .collect(),
        forward_declarations,
        definitions,
    }
    .render()?;
    Ok(indent(&source))
}

/// Re-indents the generated C by brace depth.
///
/// Codec statements nest inside each other as plain text, so their own
/// indentation carries no meaning. The generated source must stay readable
/// for the people who debug it. A line that continues a string literal after
/// a backslash stays as written, because its leading spaces are part of the
/// string.
fn indent(source: &str) -> String {
    let mut depth = 0usize;
    let mut indented = String::with_capacity(source.len());
    let mut previous_blank = true;
    let mut string_continues = false;
    for raw in source.lines() {
        if string_continues {
            let scan = Scan::line(raw, Some('"'));
            indented.push_str(raw);
            indented.push('\n');
            depth = (depth + scan.opens).saturating_sub(scan.closes);
            string_continues = scan.continues_string(raw);
            continue;
        }
        let line = raw.trim();
        if line.is_empty() && previous_blank {
            continue;
        }
        previous_blank = line.is_empty();
        let scan = Scan::line(line, None);
        let leading_closes = line
            .chars()
            .take_while(|character| *character == '}')
            .count();
        let line_depth = depth.saturating_sub(leading_closes.min(scan.closes));
        if !line.is_empty() && !line.starts_with('#') {
            (0..line_depth).for_each(|_| indented.push_str("    "));
        }
        indented.push_str(line);
        indented.push('\n');
        depth = (depth + scan.opens).saturating_sub(scan.closes);
        string_continues = scan.continues_string(line);
    }
    indented
}

/// The braces on one line of C outside string and character literals.
struct Scan {
    opens: usize,
    closes: usize,
    /// The quote still open at the end of the line.
    quote: Option<char>,
}

impl Scan {
    /// Scans `line`, which starts inside `quote` when a literal continues.
    fn line(line: &str, quote: Option<char>) -> Self {
        let mut scan = Self {
            opens: 0,
            closes: 0,
            quote,
        };
        let mut escaped = false;
        for character in line.chars() {
            match (scan.quote, character) {
                (Some(_), _) if escaped => escaped = false,
                (Some(_), '\\') => escaped = true,
                (Some(delimiter), _) if character == delimiter => scan.quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => scan.quote = Some(character),
                (None, '{') => scan.opens += 1,
                (None, '}') => scan.closes += 1,
                (None, _) => {}
            }
        }
        scan
    }

    /// A string literal that is open at a final backslash continues on the
    /// next line. A stray quote, such as the one in `/* don't */`, does not.
    fn continues_string(&self, line: &str) -> bool {
        self.quote == Some('"') && line.ends_with('\\')
    }
}

#[cfg(test)]
mod tests {
    use super::indent;

    #[test]
    fn indent_nests_blocks_and_keeps_continued_string_literals() {
        let source = "/* don't */\nvoid f(void) {\nif (x) {\ny();\n}\n}\nconst char *text = \"a\\\n    b {\";\nvoid g(void) {\nreturn;\n}\n";

        assert_eq!(
            indent(source),
            "/* don't */\nvoid f(void) {\n    if (x) {\n        y();\n    }\n}\nconst char *text = \"a\\\n    b {\";\nvoid g(void) {\n    return;\n}\n"
        );
    }
}

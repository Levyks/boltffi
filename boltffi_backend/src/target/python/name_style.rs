use boltffi_binding::{CanonicalName, NamePart};

pub struct Name<'source> {
    source: &'source CanonicalName,
}

impl<'source> Name<'source> {
    pub fn new(source: &'source CanonicalName) -> Self {
        Self { source }
    }

    pub fn function(&self) -> String {
        let name = self
            .source
            .parts()
            .iter()
            .map(NamePart::as_str)
            .collect::<Vec<_>>()
            .join("_");
        match keyword(&name) {
            true => format!("{name}_"),
            false => name,
        }
    }
}

fn keyword(name: &str) -> bool {
    matches!(
        name,
        "False"
            | "None"
            | "True"
            | "and"
            | "as"
            | "assert"
            | "async"
            | "await"
            | "break"
            | "class"
            | "continue"
            | "def"
            | "del"
            | "elif"
            | "else"
            | "except"
            | "finally"
            | "for"
            | "from"
            | "global"
            | "if"
            | "import"
            | "in"
            | "is"
            | "lambda"
            | "nonlocal"
            | "not"
            | "or"
            | "pass"
            | "raise"
            | "return"
            | "try"
            | "while"
            | "with"
            | "yield"
    )
}

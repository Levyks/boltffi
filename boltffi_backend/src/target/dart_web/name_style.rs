use boltffi_binding::{CanonicalName, FieldKey};

use crate::core::{LanguageSyntax, name_case};

pub fn lower_camel(name: &CanonicalName) -> String {
    escape(name_case::lower_camel(name))
}

pub fn upper_camel(name: &CanonicalName) -> String {
    name_case::upper_camel(name)
}

pub fn field(key: &FieldKey) -> String {
    match key {
        FieldKey::Named(name) => lower_camel(name),
        FieldKey::Position(position) => format!("field{position}"),
        _ => unreachable!("unknown field key"),
    }
}

fn escape(value: String) -> String {
    escape_identifier(value)
}

/// Escapes a raw identifier that would be illegal as Dart source (keywords).
pub fn escape_identifier(value: impl AsRef<str>) -> String {
    let value = value.as_ref();
    if super::syntax::Syntax::KEYWORDS.contains(&value) {
        format!("{value}_")
    } else {
        value.to_owned()
    }
}

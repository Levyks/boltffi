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

pub fn binder(raw: u32) -> String {
    format!("boltffiValue{raw}")
}

fn escape(value: String) -> String {
    if super::syntax::Syntax::KEYWORDS.contains(&value.as_str()) {
        format!("{value}_")
    } else {
        value
    }
}

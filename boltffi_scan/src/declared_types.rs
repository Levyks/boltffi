use std::collections::HashMap;

use boltffi_ast::{EnumId, RecordId, TraitId};

use crate::marked::MarkedItems;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum DeclaredType {
    Record(RecordId),
    Enum(EnumId),
    Trait(TraitId),
}

#[derive(Clone, Debug, Default)]
pub(super) struct DeclaredTypes {
    by_path: HashMap<String, DeclaredType>,
}

impl DeclaredTypes {
    #[cfg(test)]
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn index(marked: &MarkedItems<'_>) -> Self {
        marked
            .records()
            .iter()
            .map(|marked| {
                DeclaredType::Record(RecordId::new(
                    marked.module().qualified(&marked.item().ident.to_string()),
                ))
            })
            .chain(marked.enums().iter().map(|marked| {
                DeclaredType::Enum(EnumId::new(
                    marked.module().qualified(&marked.item().ident.to_string()),
                ))
            }))
            .chain(marked.traits().iter().map(|marked| {
                DeclaredType::Trait(TraitId::new(
                    marked.module().qualified(&marked.item().ident.to_string()),
                ))
            }))
            .fold(Self::default(), |mut declared_types, declared_type| {
                declared_types.register(declared_type);
                declared_types
            })
    }

    #[cfg(test)]
    pub(super) fn register_record(&mut self, id: RecordId) {
        self.register(DeclaredType::Record(id));
    }

    #[cfg(test)]
    pub(super) fn register_enum(&mut self, id: EnumId) {
        self.register(DeclaredType::Enum(id));
    }

    #[cfg(test)]
    pub(super) fn register_trait(&mut self, id: TraitId) {
        self.register(DeclaredType::Trait(id));
    }

    pub(super) fn resolve(&self, path: &str) -> Option<&DeclaredType> {
        self.by_path.get(path)
    }

    fn register(&mut self, declared_type: DeclaredType) {
        self.by_path
            .insert(declared_type.path().to_owned(), declared_type);
    }
}

impl DeclaredType {
    fn path(&self) -> &str {
        match self {
            Self::Record(id) => id.as_str(),
            Self::Enum(id) => id.as_str(),
            Self::Trait(id) => id.as_str(),
        }
    }
}

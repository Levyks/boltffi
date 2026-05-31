use std::collections::HashMap;

use boltffi_binding::{Bindings, DeclarationId, LoweredBindings, Surface};

use super::decl::{PairedDeclaration, SourceDeclaration};
use super::error::Error;

pub struct ExpansionIndex {
    binding_by_id: HashMap<DeclarationId, usize>,
}

impl ExpansionIndex {
    pub fn new<S: Surface>(bindings: &Bindings<S>) -> Self {
        Self {
            binding_by_id: bindings
                .decls()
                .iter()
                .enumerate()
                .map(|(index, decl)| (decl.id(), index))
                .collect(),
        }
    }

    pub fn paired<'a, S: Surface>(
        &self,
        lowered: &'a LoweredBindings<S>,
        source: SourceDeclaration<'a>,
    ) -> Result<PairedDeclaration<'a, S>, Error> {
        let source_id = source.id();
        let binding_id = lowered
            .declarations()
            .get(&source_id)
            .ok_or_else(|| Error::MissingBinding(source_id.clone()))?;
        let binding_index = self
            .binding_by_id
            .get(&binding_id)
            .copied()
            .ok_or(Error::MissingDeclaration(binding_id))?;
        let binding = lowered
            .bindings()
            .decls()
            .get(binding_index)
            .ok_or(Error::MissingDeclaration(binding_id))?;
        source.pair(binding)
    }
}

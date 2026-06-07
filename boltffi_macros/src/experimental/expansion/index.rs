use std::collections::HashMap;

use boltffi_binding::{
    CallbackDecl, CallbackId, CustomTypeDecl, CustomTypeId, Decl, DeclarationId, EncodedRecordDecl,
    LoweredBindings, RecordDecl, RecordId, Surface,
};

use super::pair::{PairedDeclaration, SourceDeclaration};
use crate::experimental::error::Error;

pub struct ExpansionIndex {
    binding_by_id: HashMap<DeclarationId, usize>,
}

impl ExpansionIndex {
    pub fn new<S: Surface>(lowered: &LoweredBindings<S>) -> Self {
        let declarations = lowered.bindings().decls();
        Self {
            binding_by_id: declarations
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

    pub fn custom_type<'a, S: Surface>(
        &self,
        lowered: &'a LoweredBindings<S>,
        id: CustomTypeId,
    ) -> Result<&'a CustomTypeDecl, Error> {
        let declaration_id = DeclarationId::CustomType(id);
        let index = self
            .binding_by_id
            .get(&declaration_id)
            .copied()
            .ok_or(Error::MissingDeclaration(declaration_id))?;
        match lowered.bindings().decls().get(index) {
            Some(Decl::CustomType(custom)) => Ok(custom),
            _ => Err(Error::WrongDeclaration),
        }
    }

    pub fn callback<'a, S: Surface>(
        &self,
        lowered: &'a LoweredBindings<S>,
        id: CallbackId,
    ) -> Result<&'a CallbackDecl<S>, Error> {
        let declaration_id = DeclarationId::Callback(id);
        let index = self
            .binding_by_id
            .get(&declaration_id)
            .copied()
            .ok_or(Error::MissingDeclaration(declaration_id))?;
        match lowered.bindings().decls().get(index) {
            Some(Decl::Callback(callback)) => Ok(callback),
            _ => Err(Error::WrongDeclaration),
        }
    }

    pub fn encoded_record<'a, S: Surface>(
        &self,
        lowered: &'a LoweredBindings<S>,
        id: RecordId,
    ) -> Result<&'a EncodedRecordDecl<S>, Error> {
        let declaration_id = DeclarationId::Record(id);
        let index = self
            .binding_by_id
            .get(&declaration_id)
            .copied()
            .ok_or(Error::MissingDeclaration(declaration_id))?;
        match lowered.bindings().decls().get(index) {
            Some(Decl::Record(record)) => match record.as_ref() {
                RecordDecl::Encoded(record) => Ok(record),
                _ => Err(Error::WrongDeclaration),
            },
            _ => Err(Error::WrongDeclaration),
        }
    }
}

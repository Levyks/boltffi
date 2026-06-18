use std::collections::HashMap;

use boltffi_ast::{
    ClassDef as SourceClass, ClassId as SourceClassId, CustomTypeDef as SourceCustom,
    CustomTypeId as SourceCustomId, EnumDef as SourceEnum, EnumId as SourceEnumId,
    FunctionDef as SourceFunction, RecordDef as SourceRecord, RecordId as SourceRecordId,
    SourceContract, TraitDef as SourceTrait, TraitId as SourceTraitId,
};

/// Borrowed view over a [`SourceContract`] with lookup tables for the
/// declarations the lowering pass dereferences while walking type
/// expressions.
///
/// The pass needs to read the source record or enum behind a
/// `TypeExpr::Record(id)` or `TypeExpr::Enum(id)` to decide whether a
/// nested reference codes as direct memory or encoded bytes. Storing
/// references rather than copying the source keeps construction cheap
/// and ties every lookup to the lifetime of the input.
pub(super) struct Index<'src> {
    source: &'src SourceContract,
    records: HashMap<&'src str, &'src SourceRecord>,
    enums: HashMap<&'src str, &'src SourceEnum>,
    classes: HashMap<&'src str, &'src SourceClass>,
    traits: HashMap<&'src str, &'src SourceTrait>,
    customs: HashMap<&'src str, &'src SourceCustom>,
}

impl<'src> Index<'src> {
    pub(super) fn new(source: &'src SourceContract) -> Self {
        Self {
            source,
            records: source
                .records
                .iter()
                .map(|record| (record.id.as_str(), record))
                .collect(),
            enums: source
                .enums
                .iter()
                .map(|enumeration| (enumeration.id.as_str(), enumeration))
                .collect(),
            classes: source
                .classes
                .iter()
                .map(|class| (class.id.as_str(), class))
                .collect(),
            traits: source
                .traits
                .iter()
                .map(|r#trait| (r#trait.id.as_str(), r#trait))
                .collect(),
            customs: source
                .customs
                .iter()
                .map(|custom| (custom.id.as_str(), custom))
                .collect(),
        }
    }

    pub(super) fn source(&self) -> &'src SourceContract {
        self.source
    }

    pub(super) fn records(&self) -> &'src [SourceRecord] {
        &self.source.records
    }

    pub(super) fn enums(&self) -> &'src [SourceEnum] {
        &self.source.enums
    }

    pub(super) fn classes(&self) -> &'src [SourceClass] {
        &self.source.classes
    }

    pub(super) fn traits(&self) -> &'src [SourceTrait] {
        &self.source.traits
    }

    pub(super) fn functions(&self) -> &'src [SourceFunction] {
        &self.source.functions
    }

    pub(super) fn constants(&self) -> &'src [boltffi_ast::ConstantDef] {
        &self.source.constants
    }

    pub(super) fn customs(&self) -> &'src [boltffi_ast::CustomTypeDef] {
        &self.source.customs
    }

    pub(super) fn streams(&self) -> &'src [boltffi_ast::StreamDef] {
        &self.source.streams
    }

    pub(super) fn record(&self, id: &SourceRecordId) -> Option<&'src SourceRecord> {
        self.records.get(id.as_str()).copied()
    }

    pub(super) fn enumeration(&self, id: &SourceEnumId) -> Option<&'src SourceEnum> {
        self.enums.get(id.as_str()).copied()
    }

    pub(super) fn class(&self, id: &SourceClassId) -> Option<&'src SourceClass> {
        self.classes.get(id.as_str()).copied()
    }

    pub(super) fn r#trait(&self, id: &SourceTraitId) -> Option<&'src SourceTrait> {
        self.traits.get(id.as_str()).copied()
    }

    pub(super) fn custom(&self, id: &SourceCustomId) -> Option<&'src SourceCustom> {
        self.customs.get(id.as_str()).copied()
    }
}

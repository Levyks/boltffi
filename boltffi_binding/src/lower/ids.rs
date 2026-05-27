use std::collections::HashMap;

use boltffi_ast::{
    ClassId as SourceClassId, ConstantId as SourceConstantId, CustomTypeId as SourceCustomTypeId,
    EnumId as SourceEnumId, FunctionId as SourceFunctionId, RecordId as SourceRecordId,
    SourceContract, StreamId as SourceStreamId, TraitId as SourceTraitId,
};

use crate::{
    CallbackId, ClassId, ConstantId, CustomTypeId, EnumId, FunctionId, RecordId, StreamId,
};

use super::{LowerError, error::DeclarationFamily};

/// Two-way mapping between source declaration ids and the typed binding
/// ids the IR carries.
///
/// Built once before the pass walks any declaration. Source contracts
/// with two declarations sharing one id in the same family fail
/// construction, so a successful build proves every later lookup that
/// hits a known id will resolve.
pub(super) struct DeclarationIds {
    records: HashMap<String, RecordId>,
    enums: HashMap<String, EnumId>,
    classes: HashMap<String, ClassId>,
    callbacks: HashMap<String, CallbackId>,
    customs: HashMap<String, CustomTypeId>,
    constants: HashMap<String, ConstantId>,
    streams: HashMap<String, StreamId>,
    functions: HashMap<String, FunctionId>,
}

impl DeclarationIds {
    pub(super) fn from_source(source: &SourceContract) -> Result<Self, LowerError> {
        Ok(Self {
            records: collect_ids(
                source.records.iter(),
                DeclarationFamily::Records,
                |record| record.id.as_str(),
                RecordId::from_raw,
            )?,
            enums: collect_ids(
                source.enums.iter(),
                DeclarationFamily::Enums,
                |enumeration| enumeration.id.as_str(),
                EnumId::from_raw,
            )?,
            classes: collect_ids(
                source.classes.iter(),
                DeclarationFamily::Classes,
                |class| class.id.as_str(),
                ClassId::from_raw,
            )?,
            callbacks: collect_ids(
                source.traits.iter(),
                DeclarationFamily::Traits,
                |source_trait| source_trait.id.as_str(),
                CallbackId::from_raw,
            )?,
            customs: collect_ids(
                source.customs.iter(),
                DeclarationFamily::CustomTypes,
                |custom| custom.id.as_str(),
                CustomTypeId::from_raw,
            )?,
            constants: collect_ids(
                source.constants.iter(),
                DeclarationFamily::Constants,
                |constant| constant.id.as_str(),
                ConstantId::from_raw,
            )?,
            streams: collect_ids(
                source.streams.iter(),
                DeclarationFamily::Streams,
                |stream| stream.id.as_str(),
                StreamId::from_raw,
            )?,
            functions: collect_ids(
                source.functions.iter(),
                DeclarationFamily::Functions,
                |function| function.id.as_str(),
                FunctionId::from_raw,
            )?,
        })
    }

    pub(super) fn record(&self, id: &SourceRecordId) -> Result<RecordId, LowerError> {
        self.records
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_record(id))
    }

    pub(super) fn enumeration(&self, id: &SourceEnumId) -> Result<EnumId, LowerError> {
        self.enums
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_enum(id))
    }

    pub(super) fn class(&self, id: &SourceClassId) -> Result<ClassId, LowerError> {
        self.classes
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_class(id))
    }

    pub(super) fn callback(&self, id: &SourceTraitId) -> Result<CallbackId, LowerError> {
        self.callbacks
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_callback(id))
    }

    pub(super) fn custom(&self, id: &SourceCustomTypeId) -> Result<CustomTypeId, LowerError> {
        self.customs
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_custom(id))
    }

    pub(super) fn constant(&self, id: &SourceConstantId) -> Result<ConstantId, LowerError> {
        self.constants
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_constant(id))
    }

    pub(super) fn stream(&self, id: &SourceStreamId) -> Result<StreamId, LowerError> {
        self.streams
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_stream(id))
    }

    pub(super) fn function(&self, id: &SourceFunctionId) -> Result<FunctionId, LowerError> {
        self.functions
            .get(id.as_str())
            .copied()
            .ok_or_else(|| LowerError::unknown_function(id))
    }
}

fn collect_ids<'item, Item, Id>(
    items: impl Iterator<Item = &'item Item>,
    family: DeclarationFamily,
    source_id: impl Fn(&Item) -> &str,
    binding_id: impl Fn(u32) -> Id,
) -> Result<HashMap<String, Id>, LowerError>
where
    Item: 'item,
{
    items
        .enumerate()
        .try_fold(HashMap::new(), |mut ids, (index, item)| {
            let id = source_id(item).to_owned();
            match ids.insert(id.clone(), binding_id(index as u32)) {
                Some(_) => Err(LowerError::duplicate_source_id(family, id)),
                None => Ok(ids),
            }
        })
}

#[cfg(test)]
mod tests {
    use boltffi_ast::{
        CanonicalName as SourceName, PackageInfo as SourcePackage, RecordDef, SourceContract,
    };

    use super::super::{DeclarationFamily, LowerErrorKind};
    use super::DeclarationIds;

    fn package() -> SourceContract {
        SourceContract::new(SourcePackage::new("demo", Some("0.1.0".to_owned())))
    }

    fn name(part: &str) -> SourceName {
        SourceName::single(part)
    }

    fn record(id: &str, record_name: &str) -> RecordDef {
        RecordDef::new(id.into(), name(record_name))
    }

    #[test]
    fn rejects_duplicate_record_source_ids() {
        let mut contract = package();
        contract.records.push(record("demo::Point", "point"));
        contract.records.push(record("demo::Point", "point_copy"));

        let error = match DeclarationIds::from_source(&contract) {
            Ok(_) => panic!("duplicate id should fail"),
            Err(error) => error,
        };

        match error.kind() {
            LowerErrorKind::DuplicateSourceId { family, id } => {
                assert_eq!(*family, DeclarationFamily::Records);
                assert_eq!(id, "demo::Point");
            }
            other => panic!("expected duplicate record id error, got {other:?}"),
        }
    }
}

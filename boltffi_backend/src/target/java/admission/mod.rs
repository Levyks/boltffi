mod enumeration;
mod function;
mod record;

use std::collections::{BTreeMap, BTreeSet};

use boltffi_binding::{Bindings, Decl, DeclarationRef, Native};

use crate::core::{
    BindingCapability, CoverageReport, DeclarationLabel, HostCapabilities, Result,
    UnsupportedDeclaration,
};

use super::JavaHost;
pub use function::{FunctionShape, ReceiverSupport};
pub use record::RecordShape;

pub struct Selection {
    bindings: Bindings<Native>,
    coverage: CoverageReport,
}

enum Decision {
    Accepted,
    Rejected(String),
}

impl Selection {
    pub fn new(host: &JavaHost, bindings: &Bindings<Native>) -> Result<Self> {
        let capabilities = host.capabilities();
        let (requested, mut reasons) = bindings.decls().iter().fold(
            (BTreeSet::new(), BTreeMap::new()),
            |(mut requested, mut reasons), declaration| {
                match Decision::for_declaration(declaration, &capabilities) {
                    Decision::Accepted => {
                        requested.insert(declaration.id());
                    }
                    Decision::Rejected(reason) => {
                        reasons.insert(declaration.id(), reason);
                    }
                }
                (requested, reasons)
            },
        );
        let retained = bindings.dependency_closed(&requested).map_err(|_| {
            JavaHost::broken_bridge_contract(
                "Java admission selects ids from the active binding contract",
            )
        })?;
        let retained_ids = retained
            .decls()
            .iter()
            .map(Decl::id)
            .collect::<BTreeSet<_>>();
        requested
            .difference(&retained_ids)
            .copied()
            .for_each(|declaration| {
                reasons.insert(
                    declaration,
                    "required declaration is unsupported".to_owned(),
                );
            });
        let coverage =
            bindings
                .decls()
                .iter()
                .fold(CoverageReport::new(), |mut coverage, declaration| {
                    if let Some(reason) = reasons.remove(&declaration.id()) {
                        coverage.push(UnsupportedDeclaration::new(
                            DeclarationLabel::from_ref(DeclarationRef::from(declaration)),
                            reason,
                        ));
                    }
                    coverage
                });
        Ok(Self {
            bindings: retained,
            coverage,
        })
    }

    pub fn into_parts(self) -> (Bindings<Native>, CoverageReport) {
        (self.bindings, self.coverage)
    }
}

impl Decision {
    fn for_declaration(declaration: &Decl<Native>, capabilities: &HostCapabilities) -> Self {
        let status = capabilities.status(BindingCapability::from_decl(declaration));
        if !status.renderable_in_partial() {
            return Self::Rejected(status.reason().to_owned());
        }
        match DeclarationRef::from(declaration) {
            DeclarationRef::Function(function) => FunctionShape::classify(function)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Record(record) => RecordShape::classify(record)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            DeclarationRef::Enum(enumeration) => EnumShape::classify(enumeration)
                .unsupported_reason()
                .map_or(Self::Accepted, |reason| Self::Rejected(reason.to_owned())),
            _ => Self::Accepted,
        }
    }
}
pub use enumeration::EnumShape;

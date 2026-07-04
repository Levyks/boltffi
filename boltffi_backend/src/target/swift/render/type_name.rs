use boltffi_binding::{EnumId, Native, Primitive, RecordId};

use crate::{
    core::{Error, RenderContext, Result},
    target::swift::{SwiftHost, name_style::Name, primitive::SwiftPrimitive, syntax::TypeName},
};

pub struct SwiftType;

impl SwiftType {
    pub fn primitive(primitive: Primitive) -> Result<TypeName> {
        SwiftPrimitive::new(primitive).api_type()
    }

    pub fn record(id: RecordId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .record(id)
            .map(|record| Name::new(record.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing record type in Swift render context",
            })
    }

    pub fn enumeration(id: EnumId, context: &RenderContext<Native>) -> Result<TypeName> {
        context
            .enumeration(id)
            .map(|enumeration| Name::new(enumeration.name()).type_name())
            .ok_or(Error::BrokenBridgeContract {
                bridge: SwiftHost::TARGET,
                invariant: "missing enum type in Swift render context",
            })
    }
}

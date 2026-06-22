use crate::{
    bridge::c::{self, Identifier},
    core::Result,
};

/// Mutation writeback for a direct record JNI parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RecordWriteback {
    local: Identifier,
}

impl RecordWriteback {
    /// Returns the local C record value written back into the Java byte array.
    pub fn local(&self) -> &Identifier {
        &self.local
    }

    pub(in crate::bridge::jni::contract::record) fn from_c_parameter(
        parameter: &c::Parameter,
    ) -> Result<Self> {
        let output = Identifier::escape(parameter.name())?;
        Ok(Self {
            local: Identifier::parse(format!("__boltffi_{}", output.as_str()))?,
        })
    }
}

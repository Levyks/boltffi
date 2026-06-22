use crate::{
    bridge::c::{self, Identifier, TypeFragment},
    core::Result,
};

/// Direct record value carried through JNI as a fixed-size byte array.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct RecordValue {
    c_type: Identifier,
}

impl RecordValue {
    /// Returns the C typedef used by the C bridge.
    pub fn c_type(&self) -> &Identifier {
        &self.c_type
    }

    /// Returns the C typedef as a type fragment.
    pub fn c_type_fragment(&self) -> TypeFragment {
        TypeFragment::new(self.c_type.to_string())
    }

    /// Creates a direct-record JNI value from a C ABI type.
    pub fn from_c_type(ty: &c::Type) -> Option<Self> {
        match ty {
            c::Type::DirectRecord(c_type) => Some(Self {
                c_type: c_type.clone(),
            }),
            _ => None,
        }
    }
}

/// JNI parameter carrying one direct record value.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct RecordParameter {
    name: Identifier,
    value: RecordValue,
    local: Identifier,
}

impl RecordParameter {
    /// Returns the generated JNI byte-array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the C typedef used by the C bridge.
    pub fn c_type(&self) -> &Identifier {
        self.value.c_type()
    }

    /// Returns the local C record value passed to the C bridge.
    pub fn local(&self) -> &Identifier {
        &self.local
    }

    /// Creates a record parameter from one direct-record C ABI parameter.
    pub fn from_c_parameter(parameter: &c::Parameter) -> Result<Option<Self>> {
        let Some(value) = RecordValue::from_c_type(parameter.ty()) else {
            return Ok(None);
        };
        let name = Identifier::escape(parameter.name())?;
        Ok(Some(Self {
            local: Identifier::parse(format!("__boltffi_{}_value", name.as_str()))?,
            name,
            value,
        }))
    }
}

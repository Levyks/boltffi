use crate::{
    bridge::c::{self, Identifier, TypeFragment},
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

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
    writeback: Option<RecordWriteback>,
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

    /// Returns the mutation writeback contract when the C bridge expects one.
    pub fn writeback(&self) -> Option<&RecordWriteback> {
        self.writeback.as_ref()
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
            writeback: None,
        }))
    }

    /// Creates a record parameter from one direct-record C ABI writeback group.
    pub fn from_c_writeback(
        writeback: &c::DirectWritebackParameter,
        function: &c::Function,
    ) -> Result<Self> {
        let input = function.parameter(writeback.input());
        let Some(value) = RecordValue::from_c_type(input.ty()) else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "direct writeback input is not a direct record",
            });
        };
        let name = Identifier::escape(writeback.name())?;
        Ok(Self {
            local: Identifier::parse(format!("__boltffi_{}_value", name.as_str()))?,
            name,
            value,
            writeback: Some(RecordWriteback::from_c_parameter(
                function.parameter(writeback.output()),
            )?),
        })
    }
}

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

    fn from_c_parameter(parameter: &c::Parameter) -> Result<Self> {
        let output = Identifier::escape(parameter.name())?;
        Ok(Self {
            local: Identifier::parse(format!("__boltffi_{}", output.as_str()))?,
        })
    }
}

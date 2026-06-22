use crate::{
    bridge::c::{self, Identifier},
    core::Result,
};

/// Byte-array JNI parameter mapped to pointer and length C bridge arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct BytesParameter {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
}

impl BytesParameter {
    /// Returns the generated JNI byte-array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the local pointer variable passed to the C bridge.
    pub fn pointer(&self) -> &Identifier {
        &self.pointer
    }

    /// Returns the local length variable passed to the C bridge.
    pub fn length(&self) -> &Identifier {
        &self.length
    }

    /// Creates a byte-array parameter from matching C pointer and length parameters.
    pub fn from_pair(
        pointer: &c::Parameter,
        length: Option<&c::Parameter>,
    ) -> Result<Option<Self>> {
        let Some(length) = length else {
            return Ok(None);
        };
        if !Self::is_pointer(pointer.ty()) || !Self::is_length(length.ty()) {
            return Ok(None);
        }
        let Some(name) = pointer.name().strip_suffix("_ptr") else {
            return Ok(None);
        };
        if length.name() != format!("{name}_len") {
            return Ok(None);
        }
        Self::new(name).map(Some)
    }

    fn new(name: &str) -> Result<Self> {
        let name = Identifier::escape(name)?;
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", name.as_str()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", name.as_str()))?,
            name,
        })
    }

    fn is_pointer(ty: &c::Type) -> bool {
        matches!(ty, c::Type::ConstPointer(inner) if matches!(inner.as_ref(), c::Type::Uint8))
    }

    fn is_length(ty: &c::Type) -> bool {
        matches!(ty, c::Type::PointerWidth)
    }
}

use boltffi_binding::{DirectVectorElementType, Primitive, Receive};

use crate::core::{Error, Result};

use super::super::syntax::{Identifier, StringLiteral, TypeName};
use super::Type;

pub struct DirectVector {
    kind: PrimitiveVector,
    receive: Option<Receive>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PrimitiveVector {
    Bool,
    I8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
}

impl DirectVector {
    pub fn new(element: &DirectVectorElementType, receive: Receive) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Ok(Self {
                kind: PrimitiveVector::new(primitive.primitive())?,
                receive: Some(receive),
            }),
            DirectVectorElementType::Record(_) => Self::unsupported("direct record vector"),
            _ => Self::unsupported("unknown direct vector"),
        }
    }

    pub fn outgoing(element: &DirectVectorElementType) -> Result<Self> {
        match element {
            DirectVectorElementType::Primitive(primitive) => Ok(Self {
                kind: PrimitiveVector::new(primitive.primitive())?,
                receive: None,
            }),
            DirectVectorElementType::Record(_) => Self::unsupported("direct record vector"),
            _ => Self::unsupported("unknown direct vector"),
        }
    }

    pub fn parameter_type(&self) -> Result<TypeName> {
        let scalar = Type::primitive(self.kind.primitive())?;
        match self.receive {
            Some(Receive::ByValue | Receive::ByRef)
                if matches!(self.kind, PrimitiveVector::Bool) =>
            {
                Ok(TypeName::readonly_array(scalar))
            }
            Some(Receive::ByValue | Receive::ByRef) => Ok(TypeName::union(
                TypeName::readonly_array(scalar),
                self.kind.typed_array(),
            )),
            Some(Receive::ByMutRef) if !matches!(self.kind, PrimitiveVector::Bool) => {
                Ok(self.kind.typed_array())
            }
            Some(Receive::ByMutRef) => Self::unsupported("mutable boolean slice"),
            None => Ok(self.return_type()),
            _ => Self::unsupported("unknown direct vector receive mode"),
        }
    }

    pub fn return_type(&self) -> TypeName {
        match self.kind {
            PrimitiveVector::Bool => TypeName::array(TypeName::boolean()),
            _ => self.kind.typed_array(),
        }
    }

    pub fn allocation_method(&self) -> Identifier {
        Identifier::known(self.kind.allocation_method())
    }

    pub fn take_method(&self) -> Identifier {
        Identifier::known(self.kind.take_method())
    }

    pub fn writeback(&self) -> bool {
        matches!(self.receive, Some(Receive::ByMutRef))
    }

    pub fn borrow_method(&self) -> Identifier {
        Identifier::known(self.kind.borrow_method())
    }

    pub const fn alignment(&self) -> usize {
        self.kind.alignment()
    }

    pub fn element_literal(&self) -> StringLiteral {
        StringLiteral::new(self.kind.element_name())
    }

    fn unsupported<T>(shape: &'static str) -> Result<T> {
        Err(Error::UnsupportedTarget {
            target: "typescript",
            shape,
        })
    }
}

impl PrimitiveVector {
    fn new(primitive: Primitive) -> Result<Self> {
        Ok(match primitive {
            Primitive::Bool => Self::Bool,
            Primitive::I8 => Self::I8,
            Primitive::I16 => Self::I16,
            Primitive::U16 => Self::U16,
            Primitive::I32 | Primitive::ISize => Self::I32,
            Primitive::U32 | Primitive::USize => Self::U32,
            Primitive::I64 => Self::I64,
            Primitive::U64 => Self::U64,
            Primitive::F32 => Self::F32,
            Primitive::F64 => Self::F64,
            Primitive::U8 => return DirectVector::unsupported("u8 direct vector"),
            _ => return DirectVector::unsupported("unknown direct vector primitive"),
        })
    }

    fn primitive(self) -> Primitive {
        match self {
            Self::Bool => Primitive::Bool,
            Self::I8 => Primitive::I8,
            Self::I16 => Primitive::I16,
            Self::U16 => Primitive::U16,
            Self::I32 => Primitive::I32,
            Self::U32 => Primitive::U32,
            Self::I64 => Primitive::I64,
            Self::U64 => Primitive::U64,
            Self::F32 => Primitive::F32,
            Self::F64 => Primitive::F64,
        }
    }

    fn typed_array(self) -> TypeName {
        TypeName::named(match self {
            Self::Bool => "Uint8Array",
            Self::I8 => "Int8Array",
            Self::I16 => "Int16Array",
            Self::U16 => "Uint16Array",
            Self::I32 => "Int32Array",
            Self::U32 => "Uint32Array",
            Self::I64 => "BigInt64Array",
            Self::U64 => "BigUint64Array",
            Self::F32 => "Float32Array",
            Self::F64 => "Float64Array",
        })
    }

    fn allocation_method(self) -> &'static str {
        match self {
            Self::Bool => "allocBoolArray",
            Self::I8 => "allocI8Array",
            Self::I16 => "allocI16Array",
            Self::U16 => "allocU16Array",
            Self::I32 => "allocI32Array",
            Self::U32 => "allocU32Array",
            Self::I64 => "allocI64Array",
            Self::U64 => "allocU64Array",
            Self::F32 => "allocF32Array",
            Self::F64 => "allocF64Array",
        }
    }

    fn take_method(self) -> &'static str {
        match self {
            Self::Bool => "takeSlotBoolArray",
            Self::I8 => "takeSlotI8Array",
            Self::I16 => "takeSlotI16Array",
            Self::U16 => "takeSlotU16Array",
            Self::I32 => "takeSlotI32Array",
            Self::U32 => "takeSlotU32Array",
            Self::I64 => "takeSlotI64Array",
            Self::U64 => "takeSlotU64Array",
            Self::F32 => "takeSlotF32Array",
            Self::F64 => "takeSlotF64Array",
        }
    }

    fn borrow_method(self) -> &'static str {
        match self {
            Self::Bool => "borrowBoolArray",
            Self::I8 => "borrowI8Array",
            Self::I16 => "borrowI16Array",
            Self::U16 => "borrowU16Array",
            Self::I32 => "borrowI32Array",
            Self::U32 => "borrowU32Array",
            Self::I64 => "borrowI64Array",
            Self::U64 => "borrowU64Array",
            Self::F32 => "borrowF32Array",
            Self::F64 => "borrowF64Array",
        }
    }

    const fn alignment(self) -> usize {
        match self {
            Self::Bool | Self::I8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    fn element_name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::U16 => "u16",
            Self::I32 => "i32",
            Self::U32 => "u32",
            Self::I64 => "i64",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

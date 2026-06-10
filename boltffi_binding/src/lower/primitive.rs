use boltffi_ast::{Primitive as SourcePrimitive, ReprAttr, ReprItem, TypeExpr};

use crate::{IntegerRepr, Primitive};

/// Size and alignment of a primitive whose ABI shape is fixed across
/// every supported target.
///
/// Records that contain only fixed-shape primitives are eligible for the
/// direct memory layout. Primitives with platform-dependent width
/// (`isize`/`usize`) deliberately have no [`FixedPrimitive`].
#[derive(Clone, Copy)]
pub(super) struct FixedPrimitive {
    pub(super) size: u64,
    pub(super) alignment: u64,
}

pub(super) fn fixed_primitive(type_expr: &TypeExpr) -> Option<FixedPrimitive> {
    match type_expr {
        TypeExpr::Primitive(primitive) => FixedPrimitive::from_source(*primitive),
        _ => None,
    }
}

pub(super) fn integer_repr(repr: &ReprAttr) -> Option<IntegerRepr> {
    repr.items.iter().find_map(|item| match item {
        ReprItem::Primitive(SourcePrimitive::I8) => Some(IntegerRepr::I8),
        ReprItem::Primitive(SourcePrimitive::U8) => Some(IntegerRepr::U8),
        ReprItem::Primitive(SourcePrimitive::I16) => Some(IntegerRepr::I16),
        ReprItem::Primitive(SourcePrimitive::U16) => Some(IntegerRepr::U16),
        ReprItem::Primitive(SourcePrimitive::I32) => Some(IntegerRepr::I32),
        ReprItem::Primitive(SourcePrimitive::U32) => Some(IntegerRepr::U32),
        ReprItem::Primitive(SourcePrimitive::I64) => Some(IntegerRepr::I64),
        ReprItem::Primitive(SourcePrimitive::U64) => Some(IntegerRepr::U64),
        ReprItem::Primitive(SourcePrimitive::ISize) => Some(IntegerRepr::ISize),
        ReprItem::Primitive(SourcePrimitive::USize) => Some(IntegerRepr::USize),
        _ => None,
    })
}

pub(super) fn has_repr_c(repr: &ReprAttr) -> bool {
    repr.items.iter().any(|item| matches!(item, ReprItem::C))
}

impl From<SourcePrimitive> for Primitive {
    fn from(primitive: SourcePrimitive) -> Self {
        match primitive {
            SourcePrimitive::Bool => Self::Bool,
            SourcePrimitive::I8 => Self::I8,
            SourcePrimitive::U8 => Self::U8,
            SourcePrimitive::I16 => Self::I16,
            SourcePrimitive::U16 => Self::U16,
            SourcePrimitive::I32 => Self::I32,
            SourcePrimitive::U32 => Self::U32,
            SourcePrimitive::I64 => Self::I64,
            SourcePrimitive::U64 => Self::U64,
            SourcePrimitive::ISize => Self::ISize,
            SourcePrimitive::USize => Self::USize,
            SourcePrimitive::F32 => Self::F32,
            SourcePrimitive::F64 => Self::F64,
        }
    }
}

impl FixedPrimitive {
    fn from_source(primitive: SourcePrimitive) -> Option<Self> {
        match primitive {
            SourcePrimitive::Bool | SourcePrimitive::I8 | SourcePrimitive::U8 => Some(Self {
                size: 1,
                alignment: 1,
            }),
            SourcePrimitive::I16 | SourcePrimitive::U16 => Some(Self {
                size: 2,
                alignment: 2,
            }),
            SourcePrimitive::I32 | SourcePrimitive::U32 | SourcePrimitive::F32 => Some(Self {
                size: 4,
                alignment: 4,
            }),
            SourcePrimitive::I64 | SourcePrimitive::U64 | SourcePrimitive::F64 => Some(Self {
                size: 8,
                alignment: 8,
            }),
            SourcePrimitive::ISize | SourcePrimitive::USize => None,
        }
    }
}

use boltffi_binding::{DirectFieldType, Primitive};

use crate::core::{Error, Result};

use super::DartHost;

pub fn api_type(primitive: Primitive) -> Result<&'static str> {
    match primitive {
        Primitive::Bool => Ok("bool"),
        Primitive::I8
        | Primitive::U8
        | Primitive::I16
        | Primitive::U16
        | Primitive::I32
        | Primitive::U32
        | Primitive::I64
        | Primitive::U64
        | Primitive::ISize
        | Primitive::USize => Ok("int"),
        Primitive::F32 | Primitive::F64 => Ok("double"),
        _ => unsupported("unknown primitive"),
    }
}

pub fn wire_suffix(primitive: Primitive) -> Result<&'static str> {
    match primitive {
        Primitive::Bool => Ok("Bool"),
        Primitive::I8 => Ok("I8"),
        Primitive::U8 => Ok("U8"),
        Primitive::I16 => Ok("I16"),
        Primitive::U16 => Ok("U16"),
        Primitive::I32 => Ok("I32"),
        Primitive::U32 => Ok("U32"),
        Primitive::I64 | Primitive::ISize => Ok("I64"),
        Primitive::U64 | Primitive::USize => Ok("U64"),
        Primitive::F32 => Ok("F32"),
        Primitive::F64 => Ok("F64"),
        _ => unsupported("unknown primitive wire method"),
    }
}

pub fn direct_field(ty: DirectFieldType) -> Result<String> {
    api_type(ty.primitive()).map(str::to_owned)
}

fn unsupported<T>(shape: &'static str) -> Result<T> {
    Err(Error::UnsupportedTarget {
        target: DartHost::TARGET,
        shape,
    })
}

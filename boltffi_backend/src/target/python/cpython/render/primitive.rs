use boltffi_binding::Primitive;

use crate::{
    bridge::c::{Type, syntax::TypeSyntax},
    core::{Error, Result},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Runtime {
    primitive: Primitive,
}

pub struct Support {
    runtime: Runtime,
    pub parser: &'static str,
    pub boxer: &'static str,
}

impl Support {
    pub fn new(runtime: Runtime) -> Result<Self> {
        Ok(Self {
            runtime,
            parser: runtime.parser()?,
            boxer: runtime.boxer()?,
        })
    }

    pub fn is_bool(&self) -> bool {
        self.runtime.is_bool()
    }

    pub fn is_i8(&self) -> bool {
        self.runtime.is_i8()
    }

    pub fn is_u8(&self) -> bool {
        self.runtime.is_u8()
    }

    pub fn is_i16(&self) -> bool {
        self.runtime.is_i16()
    }

    pub fn is_u16(&self) -> bool {
        self.runtime.is_u16()
    }

    pub fn is_i32(&self) -> bool {
        self.runtime.is_i32()
    }

    pub fn is_u32(&self) -> bool {
        self.runtime.is_u32()
    }

    pub fn is_i64(&self) -> bool {
        self.runtime.is_i64()
    }

    pub fn is_u64(&self) -> bool {
        self.runtime.is_u64()
    }

    pub fn is_isize(&self) -> bool {
        self.runtime.is_isize()
    }

    pub fn is_usize(&self) -> bool {
        self.runtime.is_usize()
    }

    pub fn is_f32(&self) -> bool {
        self.runtime.is_f32()
    }

    pub fn is_f64(&self) -> bool {
        self.runtime.is_f64()
    }
}

impl Runtime {
    pub fn new(primitive: Primitive) -> Self {
        Self { primitive }
    }

    pub fn c_type(self) -> Result<String> {
        TypeSyntax::new(&Type::primitive(self.primitive)?).anonymous()
    }

    pub fn parser(self) -> Result<&'static str> {
        Ok(match self.primitive {
            Primitive::Bool => "boltffi_python_parse_bool",
            Primitive::I8 => "boltffi_python_parse_i8",
            Primitive::U8 => "boltffi_python_parse_u8",
            Primitive::I16 => "boltffi_python_parse_i16",
            Primitive::U16 => "boltffi_python_parse_u16",
            Primitive::I32 => "boltffi_python_parse_i32",
            Primitive::U32 => "boltffi_python_parse_u32",
            Primitive::I64 => "boltffi_python_parse_i64",
            Primitive::U64 => "boltffi_python_parse_u64",
            Primitive::ISize => "boltffi_python_parse_isize",
            Primitive::USize => "boltffi_python_parse_usize",
            Primitive::F32 => "boltffi_python_parse_f32",
            Primitive::F64 => "boltffi_python_parse_f64",
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown primitive parser",
                });
            }
        })
    }

    pub fn boxer(self) -> Result<&'static str> {
        Ok(match self.primitive {
            Primitive::Bool => "boltffi_python_box_bool",
            Primitive::I8 => "boltffi_python_box_i8",
            Primitive::U8 => "boltffi_python_box_u8",
            Primitive::I16 => "boltffi_python_box_i16",
            Primitive::U16 => "boltffi_python_box_u16",
            Primitive::I32 => "boltffi_python_box_i32",
            Primitive::U32 => "boltffi_python_box_u32",
            Primitive::I64 => "boltffi_python_box_i64",
            Primitive::U64 => "boltffi_python_box_u64",
            Primitive::ISize => "boltffi_python_box_isize",
            Primitive::USize => "boltffi_python_box_usize",
            Primitive::F32 => "boltffi_python_box_f32",
            Primitive::F64 => "boltffi_python_box_f64",
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown primitive boxer",
                });
            }
        })
    }

    pub fn is_bool(&self) -> bool {
        matches!(self.primitive, Primitive::Bool)
    }

    pub fn is_i8(&self) -> bool {
        matches!(self.primitive, Primitive::I8)
    }

    pub fn is_u8(&self) -> bool {
        matches!(self.primitive, Primitive::U8)
    }

    pub fn is_i16(&self) -> bool {
        matches!(self.primitive, Primitive::I16)
    }

    pub fn is_u16(&self) -> bool {
        matches!(self.primitive, Primitive::U16)
    }

    pub fn is_i32(&self) -> bool {
        matches!(self.primitive, Primitive::I32)
    }

    pub fn is_u32(&self) -> bool {
        matches!(self.primitive, Primitive::U32)
    }

    pub fn is_i64(&self) -> bool {
        matches!(self.primitive, Primitive::I64)
    }

    pub fn is_u64(&self) -> bool {
        matches!(self.primitive, Primitive::U64)
    }

    pub fn is_isize(&self) -> bool {
        matches!(self.primitive, Primitive::ISize)
    }

    pub fn is_usize(&self) -> bool {
        matches!(self.primitive, Primitive::USize)
    }

    pub fn is_f32(&self) -> bool {
        matches!(self.primitive, Primitive::F32)
    }

    pub fn is_f64(&self) -> bool {
        matches!(self.primitive, Primitive::F64)
    }
}

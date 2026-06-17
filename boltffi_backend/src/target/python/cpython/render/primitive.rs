use boltffi_binding::{Primitive, native};

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
    pub wire_encoder: String,
    pub owned_wire_decoder: String,
}

impl Support {
    pub fn new(runtime: Runtime) -> Result<Self> {
        Ok(Self {
            runtime,
            parser: runtime.parser()?,
            boxer: runtime.boxer()?,
            wire_encoder: runtime.wire_encoder()?,
            owned_wire_decoder: runtime.owned_wire_decoder()?,
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

    pub fn native_handle(carrier: native::HandleCarrier) -> Result<Self> {
        let primitive = match carrier {
            native::HandleCarrier::U64 => Primitive::U64,
            native::HandleCarrier::USize => Primitive::USize,
            native::HandleCarrier::CallbackHandle => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "callback handle carrier",
                });
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown native handle carrier",
                });
            }
        };
        Ok(Self::new(primitive))
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

    pub fn wire_encoder(self) -> Result<String> {
        Ok(format!("boltffi_python_wire_{}", self.wire_stem()?))
    }

    pub fn owned_wire_decoder(self) -> Result<String> {
        Ok(format!("boltffi_python_decode_owned_{}", self.wire_stem()?))
    }

    pub fn direct_vec_decoder(self) -> Result<String> {
        Ok(format!(
            "boltffi_python_decode_owned_vec_{}",
            self.wire_stem()?
        ))
    }

    pub fn direct_vec_parser(self) -> Result<String> {
        Ok(format!("boltffi_python_parse_vec_{}", self.wire_stem()?))
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

    pub fn wire_stem(self) -> Result<&'static str> {
        Ok(match self.primitive {
            Primitive::Bool => "bool",
            Primitive::I8 => "i8",
            Primitive::U8 => "u8",
            Primitive::I16 => "i16",
            Primitive::U16 => "u16",
            Primitive::I32 => "i32",
            Primitive::U32 => "u32",
            Primitive::I64 => "i64",
            Primitive::U64 => "u64",
            Primitive::ISize => "isize",
            Primitive::USize => "usize",
            Primitive::F32 => "f32",
            Primitive::F64 => "f64",
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown primitive wire support",
                });
            }
        })
    }
}

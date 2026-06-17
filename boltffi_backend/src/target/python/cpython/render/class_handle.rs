use boltffi_binding::{Primitive, native};

use crate::{
    core::{Error, Result},
    target::python::cpython::render::primitive,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Carrier {
    primitive: primitive::Runtime,
}

impl Carrier {
    pub fn new(carrier: native::HandleCarrier) -> Result<Self> {
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
        Ok(Self {
            primitive: primitive::Runtime::new(primitive),
        })
    }

    pub fn c_type(self) -> Result<String> {
        self.primitive.c_type()
    }

    pub fn parser(self) -> Result<&'static str> {
        self.primitive.parser()
    }

    pub fn boxer(self) -> Result<&'static str> {
        self.primitive.boxer()
    }

    pub fn primitive(self) -> primitive::Runtime {
        self.primitive
    }
}

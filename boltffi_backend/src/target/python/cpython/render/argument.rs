use boltffi_binding::{
    IncomingParam, IntoRust, Native, ParamDecl, ParamPlan, Receive, TypeRef, native,
};

use crate::{
    bridge::c::identifier::Identifier,
    core::{Error, Result},
    target::python::{cpython::render::primitive, name_style::Name},
};

pub struct Conversion {
    index: usize,
    name: String,
    kind: Kind,
    primitive: Option<primitive::Runtime>,
}

impl Conversion {
    pub fn from_parameter(index: usize, parameter: &ParamDecl<Native, IntoRust>) -> Result<Self> {
        match parameter.payload() {
            IncomingParam::Value(ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                receive: Receive::ByValue,
            }) => Self::from_primitive(index, parameter, primitive::Runtime::new(*primitive)),
            IncomingParam::Value(ParamPlan::Encoded {
                ty: TypeRef::String,
                shape: native::BufferShape::Slice,
                receive,
                ..
            }) => Self::encoded(index, parameter, *receive, Encoded::String),
            IncomingParam::Value(ParamPlan::Encoded {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Slice,
                receive,
                ..
            }) => Self::encoded(index, parameter, *receive, Encoded::Bytes),
            IncomingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure parameter",
            }),
            IncomingParam::Value(ParamPlan::Direct { .. }) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "borrowed direct parameter",
            }),
            IncomingParam::Value(ParamPlan::Encoded { .. }) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported encoded parameter",
            }),
            IncomingParam::Value(
                ParamPlan::Handle { .. }
                | ParamPlan::ScalarOption { .. }
                | ParamPlan::DirectVec { .. },
            ) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter",
            }),
            IncomingParam::Value(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown parameter plan",
            }),
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    pub fn call_args(&self) -> Vec<String> {
        match &self.kind {
            Kind::Primitive(_) => vec![self.name.clone()],
            Kind::Encoded(encoded) => vec![encoded.pointer.clone(), encoded.length.clone()],
        }
    }

    pub const fn index(&self) -> usize {
        self.index
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self.kind, Kind::Primitive(_))
    }

    pub fn is_encoded(&self) -> bool {
        matches!(self.kind, Kind::Encoded(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(
            self.kind,
            Kind::Encoded(EncodedParam {
                value: Encoded::String,
                ..
            })
        )
    }

    pub fn is_bytes(&self) -> bool {
        matches!(
            self.kind,
            Kind::Encoded(EncodedParam {
                value: Encoded::Bytes,
                ..
            })
        )
    }

    pub fn c_type(&self) -> &str {
        match &self.kind {
            Kind::Primitive(primitive) => primitive.c_type.as_str(),
            Kind::Encoded(_) => "",
        }
    }

    pub fn parser(&self) -> &str {
        match &self.kind {
            Kind::Primitive(primitive) => primitive.parser,
            Kind::Encoded(encoded) => encoded.parser,
        }
    }

    pub fn wire(&self) -> &str {
        match &self.kind {
            Kind::Primitive(_) => "",
            Kind::Encoded(encoded) => encoded.wire.as_str(),
        }
    }

    pub fn pointer(&self) -> &str {
        match &self.kind {
            Kind::Primitive(_) => "",
            Kind::Encoded(encoded) => encoded.pointer.as_str(),
        }
    }

    pub fn length(&self) -> &str {
        match &self.kind {
            Kind::Primitive(_) => "",
            Kind::Encoded(encoded) => encoded.length.as_str(),
        }
    }

    fn from_primitive(
        index: usize,
        parameter: &ParamDecl<Native, IntoRust>,
        primitive: primitive::Runtime,
    ) -> Result<Self> {
        let name = Identifier::escape(Name::new(parameter.name()).function())?.to_string();
        Ok(Self {
            index,
            name,
            kind: Kind::Primitive(Primitive {
                c_type: primitive.c_type()?,
                parser: primitive.parser()?,
            }),
            primitive: Some(primitive),
        })
    }

    fn encoded(
        index: usize,
        parameter: &ParamDecl<Native, IntoRust>,
        receive: Receive,
        encoded: Encoded,
    ) -> Result<Self> {
        if matches!(receive, Receive::ByMutRef) {
            Err(Error::UnsupportedTarget {
                target: "python",
                shape: "mutable encoded parameter",
            })
        } else {
            let name = Identifier::escape(Name::new(parameter.name()).function())?.to_string();
            let wire = format!("{name}_wire");
            let pointer = format!("{name}_ptr");
            let length = format!("{name}_len");
            Ok(Self {
                index,
                name,
                kind: Kind::Encoded(EncodedParam {
                    value: encoded,
                    parser: encoded.parser(),
                    wire,
                    pointer,
                    length,
                }),
                primitive: None,
            })
        }
    }
}

enum Kind {
    Primitive(Primitive),
    Encoded(EncodedParam),
}

struct Primitive {
    c_type: String,
    parser: &'static str,
}

struct EncodedParam {
    value: Encoded,
    parser: &'static str,
    wire: String,
    pointer: String,
    length: String,
}

#[derive(Clone, Copy)]
enum Encoded {
    String,
    Bytes,
}

impl Encoded {
    fn parser(self) -> &'static str {
        match self {
            Self::String => "boltffi_python_wire_string",
            Self::Bytes => "boltffi_python_wire_bytes",
        }
    }
}

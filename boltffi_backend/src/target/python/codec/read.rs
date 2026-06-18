use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, Op, Primitive,
    RecordId,
};

use crate::{
    core::{Error, Result},
    target::python::{cpython::render::primitive, render::Package},
};

pub struct Reader<'package, 'binding, 'bridge> {
    package: &'package Package<'binding, 'bridge>,
}

impl<'package, 'binding, 'bridge> Reader<'package, 'binding, 'bridge> {
    pub fn new(package: &'package Package<'binding, 'bridge>) -> Self {
        Self { package }
    }
}

impl CodecRead for Reader<'_, '_, '_> {
    type Expr = Result<String>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        let stem = primitive::Runtime::new(primitive).wire_stem()?;
        Ok(format!("reader.{stem}()"))
    }

    fn string(&mut self) -> Self::Expr {
        Ok("reader.string()".to_owned())
    }

    fn bytes(&mut self) -> Self::Expr {
        Ok("reader.bytes()".to_owned())
    }

    fn direct_record(&mut self, id: RecordId) -> Self::Expr {
        self.encoded_record(id)
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        Ok(format!(
            "{}._boltffi_from_reader(reader)",
            self.package.record_name(id)?
        ))
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        let EnumCodec::CStyle(primitive) = self.package.enum_codec(id)? else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "data enum reached c-style wire reader",
            });
        };
        let stem = primitive::Runtime::new(primitive).wire_stem()?;
        Ok(format!("{}(reader.{stem}())", self.package.enum_name(id)?))
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        let EnumCodec::Data { class_name } = self.package.enum_codec(id)? else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "c-style enum reached data enum wire reader",
            });
        };
        Ok(format!("{class_name}._boltffi_from_reader(reader)"))
    }

    fn class_handle(&mut self, id: ClassId) -> Self::Expr {
        self.package.class_name(&id)?;
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "class handle in wire reader",
        })
    }

    fn callback_handle(&mut self, id: CallbackId) -> Self::Expr {
        id.raw();
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "callback handle in wire reader",
        })
    }

    fn custom(&mut self, id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        self.package.custom_type(id)?;
        representation
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        Ok(match kind {
            BuiltinType::Duration => "reader.duration()".to_owned(),
            BuiltinType::SystemTime => "reader.system_time()".to_owned(),
            BuiltinType::Uuid => "reader.uuid()".to_owned(),
            BuiltinType::Url => "reader.url()".to_owned(),
        })
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(format!("reader.optional(lambda: {})", inner?))
    }

    fn sequence(&mut self, len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        len.node();
        Ok(format!("reader.sequence(lambda: {})", element?))
    }

    fn tuple(&mut self, elements: Vec<Self::Expr>) -> Self::Expr {
        Ok(format!(
            "({},)",
            elements.into_iter().collect::<Result<Vec<_>>>()?.join(", ")
        ))
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(format!("reader.result(lambda: {}, lambda: {})", ok?, err?))
    }

    fn map(&mut self, key: Self::Expr, value: Self::Expr) -> Self::Expr {
        Ok(format!("reader.map(lambda: {}, lambda: {})", key?, value?))
    }
}

pub enum EnumCodec {
    CStyle(Primitive),
    Data { class_name: String },
}

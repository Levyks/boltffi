use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecWrite, CustomTypeId, ElementCount, EnumId, Op,
    Primitive, RecordId, ValueRef,
};

use crate::{
    core::{Error, Result},
    target::python::{
        codec::{operation::Operation, read::EnumCodec, value::ValueExpression},
        cpython::render::primitive,
        render::Package,
    },
};

pub struct Writer<'package, 'binding, 'bridge> {
    package: &'package Package<'binding, 'bridge>,
}

impl<'package, 'binding, 'bridge> Writer<'package, 'binding, 'bridge> {
    pub fn new(package: &'package Package<'binding, 'bridge>) -> Self {
        Self { package }
    }

    pub fn single(expressions: Vec<Result<String>>) -> Result<String> {
        let mut expressions = expressions.into_iter().collect::<Result<Vec<_>>>()?;
        match expressions.len() {
            1 => Ok(expressions.remove(0)),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "multi-statement wire writer",
            }),
        }
    }

    fn value(&self, value: &ValueRef) -> Result<String> {
        ValueExpression::new(value).render()
    }

    fn binder(binder: BinderId) -> String {
        ValueExpression::binder(binder)
    }

    fn write_primitive(&self, value: String, primitive: Primitive) -> Result<String> {
        let stem = primitive::Runtime::new(primitive).wire_stem()?;
        Ok(format!("_boltffi_wire_{stem}({value})"))
    }

    fn write_enum(&self, value: String, enumeration: EnumId) -> Result<String> {
        match self.package.enum_codec(enumeration)? {
            EnumCodec::CStyle(primitive) => {
                let stem = primitive::Runtime::new(primitive).wire_stem()?;
                let enum_name = self.package.enum_name(enumeration)?;
                let enum_name_literal = Package::literal(&enum_name);
                Ok(format!(
                    "_boltffi_wire_{stem}(_boltffi_enum_value({value}, {enum_name}, {enum_name_literal}))"
                ))
            }
            EnumCodec::Data { .. } => Ok(format!("{value}._boltffi_wire()")),
        }
    }

    fn write_builtin(value: String, builtin: BuiltinType) -> String {
        match builtin {
            BuiltinType::Duration => format!("_boltffi_wire_duration({value})"),
            BuiltinType::SystemTime => format!("_boltffi_wire_system_time({value})"),
            BuiltinType::Uuid => format!("_boltffi_wire_uuid({value})"),
            BuiltinType::Url => format!("_boltffi_wire_url({value})"),
        }
    }
}

impl CodecWrite for Writer<'_, '_, '_> {
    type Stmt = Result<String>;

    fn primitive(&mut self, primitive: Primitive, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| self.write_primitive(value, primitive)),
        ]
    }

    fn string(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .map(|value| format!("_boltffi_wire_string({value})")),
        ]
    }

    fn bytes(&mut self, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .map(|value| format!("_boltffi_wire_bytes({value})")),
        ]
    }

    fn direct_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        self.encoded_record(id, value)
    }

    fn encoded_record(&mut self, id: RecordId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            self.package
                .record_name(id)
                .map(|_| format!("{value}._boltffi_wire()"))
        })]
    }

    fn c_style_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .and_then(|value| self.write_enum(value, id)),
        ]
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            self.package
                .enum_codec(id)
                .map(|_| format!("{value}._boltffi_wire()"))
        })]
    }

    fn class_handle(&mut self, id: ClassId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|_| {
            self.package.class_name(&id)?;
            Err(Error::UnsupportedTarget {
                target: "python",
                shape: "class handle in wire writer",
            })
        })]
    }

    fn callback_handle(&mut self, id: CallbackId, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|_| {
            id.raw();
            Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback handle in wire writer",
            })
        })]
    }

    fn custom(
        &mut self,
        id: CustomTypeId,
        value: &ValueRef,
        representation: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|_| {
            self.package.custom_type(id)?;
            Self::single(representation)
        })]
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Vec<Self::Stmt> {
        vec![
            self.value(value)
                .map(|value| Self::write_builtin(value, kind)),
        ]
    }

    fn optional(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        inner: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(format!(
                "_boltffi_wire_optional({value}, lambda {}: {})",
                Self::binder(binder),
                Self::single(inner)?
            ))
        })]
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        len: &Op<ElementCount>,
        binder: BinderId,
        element: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            let count = len.render_with(&mut Operation);
            Ok(format!(
                "_boltffi_wire_sequence({value}, {}, lambda {}: {})",
                count?,
                Self::binder(binder),
                Self::single(element)?
            ))
        })]
    }

    fn tuple(&mut self, value: &ValueRef, elements: Vec<Vec<Self::Stmt>>) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|_| {
            elements
                .into_iter()
                .map(Self::single)
                .collect::<Result<Vec<_>>>()
                .map(|fields| format!("b\"\".join(({},))", fields.join(", ")))
        })]
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Vec<Self::Stmt>,
        err: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(format!(
                "_boltffi_wire_result({value}, lambda {}: {}, lambda {}: {})",
                Self::binder(binder),
                Self::single(ok)?,
                Self::binder(binder),
                Self::single(err)?
            ))
        })]
    }

    fn map(
        &mut self,
        value: &ValueRef,
        key_binder: BinderId,
        key: Vec<Self::Stmt>,
        value_binder: BinderId,
        map_value: Vec<Self::Stmt>,
    ) -> Vec<Self::Stmt> {
        vec![self.value(value).and_then(|value| {
            Ok(format!(
                "_boltffi_wire_map({value}, lambda {}: {}, lambda {}: {})",
                Self::binder(key_binder),
                Self::single(key)?,
                Self::binder(value_binder),
                Self::single(map_value)?
            ))
        })]
    }
}

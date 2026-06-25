use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CodecRead, CustomTypeId, ElementCount, EnumId, MapKind,
    Native, Op, Primitive, RecordId,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::kotlin::{
        primitive::KotlinPrimitive,
        render::{Enumeration, Record},
        syntax::{ArgumentList, Expression, Identifier},
    },
};

pub struct Reader<'context> {
    reader: Identifier,
    context: &'context RenderContext<'context, Native>,
}

impl<'context> Reader<'context> {
    pub fn new(reader: Identifier, context: &'context RenderContext<'context, Native>) -> Self {
        Self { reader, context }
    }

    fn call(&self, method: impl Into<String>) -> Result<Expression> {
        Ok(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse(method)?,
            ArgumentList::default(),
        ))
    }

    fn unsupported(shape: &'static str) -> Result<Expression> {
        Err(Error::UnsupportedTarget {
            target: "kotlin",
            shape,
        })
    }
}

impl CodecRead for Reader<'_> {
    type Expr = Result<Expression>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Expr {
        KotlinPrimitive::new(primitive)
            .wire_method_suffix()
            .and_then(|suffix| self.call(format!("read{suffix}")))
    }

    fn string(&mut self) -> Self::Expr {
        self.call("readString")
    }

    fn bytes(&mut self) -> Self::Expr {
        self.call("readBytes")
    }

    fn direct_record(&mut self, _id: RecordId) -> Self::Expr {
        Self::unsupported("direct-record wire read")
    }

    fn encoded_record(&mut self, id: RecordId) -> Self::Expr {
        Record::type_name_from_id(id, self.context).and_then(|record| {
            Ok(Expression::call(
                record,
                Identifier::parse("fromReader")?,
                [Expression::identifier(self.reader.clone())]
                    .into_iter()
                    .collect::<ArgumentList>(),
            ))
        })
    }

    fn c_style_enum(&mut self, id: EnumId) -> Self::Expr {
        Enumeration::from_id(id, self.context).and_then(|enumeration| {
            KotlinPrimitive::new(enumeration.repr()?)
                .wire_method_suffix()
                .and_then(|suffix| {
                    self.call(format!("read{suffix}")).and_then(|value| {
                        Ok(Expression::call(
                            enumeration.name().clone(),
                            Identifier::parse("fromValue")?,
                            [value].into_iter().collect::<ArgumentList>(),
                        ))
                    })
                })
        })
    }

    fn data_enum(&mut self, id: EnumId) -> Self::Expr {
        Enumeration::read_expression(id, self.reader.clone(), self.context)
    }

    fn class_handle(&mut self, _id: ClassId) -> Self::Expr {
        Self::unsupported("class handle wire read")
    }

    fn callback_handle(&mut self, _id: CallbackId) -> Self::Expr {
        Self::unsupported("callback handle wire read")
    }

    fn custom(&mut self, _id: CustomTypeId, representation: Self::Expr) -> Self::Expr {
        representation
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Expr {
        let method = match kind {
            BuiltinType::Duration => "readDuration",
            BuiltinType::SystemTime => "readInstant",
            BuiltinType::Uuid => "readUuid",
            BuiltinType::Url => "readUri",
        };
        self.call(method)
    }

    fn optional(&mut self, inner: Self::Expr) -> Self::Expr {
        Ok(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse("readOptionalValue")?,
            [Expression::lambda_expression(
                vec![self.reader.clone()],
                inner?,
            )]
            .into_iter()
            .collect::<ArgumentList>(),
        ))
    }

    fn sequence(&mut self, _len: &Op<ElementCount>, element: Self::Expr) -> Self::Expr {
        Ok(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse("readSequence")?,
            [Expression::lambda_expression(
                vec![self.reader.clone()],
                element?,
            )]
            .into_iter()
            .collect::<ArgumentList>(),
        ))
    }

    fn tuple(&mut self, _elements: Vec<Self::Expr>) -> Self::Expr {
        Self::unsupported("tuple wire read")
    }

    fn result(&mut self, ok: Self::Expr, err: Self::Expr) -> Self::Expr {
        Ok(Expression::call(
            Expression::identifier(self.reader.clone()),
            Identifier::parse("readResult")?,
            [
                Expression::lambda_expression(vec![self.reader.clone()], ok?),
                Expression::lambda_expression(vec![self.reader.clone()], err?),
            ]
            .into_iter()
            .collect::<ArgumentList>(),
        ))
    }

    fn map(&mut self, _kind: MapKind, _key: Self::Expr, _value: Self::Expr) -> Self::Expr {
        Self::unsupported("map wire read")
    }
}

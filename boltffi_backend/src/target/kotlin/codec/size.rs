use boltffi_binding::{
    BinderId, BuiltinType, CallbackId, ClassId, CodecSize, CustomTypeId, ElementCount, EnumId,
    MapKind, Native, Op, Primitive, RecordId, ValueRef,
};

use crate::{
    core::{Error, RenderContext, Result},
    target::kotlin::{
        codec::value::ValueExpression,
        primitive::KotlinPrimitive,
        render::Enumeration,
        syntax::{ArgumentList, Expression, Identifier},
    },
};

pub struct Sizer<'context> {
    current: Expression,
    context: &'context RenderContext<'context, Native>,
}

impl<'context> Sizer<'context> {
    pub fn new(context: &'context RenderContext<'context, Native>) -> Result<Self> {
        Ok(Self {
            current: Expression::identifier(Identifier::parse("value")?),
            context,
        })
    }

    pub fn current(mut self, current: Expression) -> Self {
        self.current = current;
        self
    }

    fn value(&self, value: &ValueRef) -> Result<Expression> {
        ValueExpression::new(value)?
            .current(self.current.clone())
            .render()
    }

    fn fixed(bytes: u64) -> Result<Expression> {
        Ok(Expression::integer(bytes))
    }

    fn string_size(&self, value: &ValueRef) -> Result<Expression> {
        self.string_expression_size(self.value(value)?)
    }

    fn string_expression_size(&self, value: Expression) -> Result<Expression> {
        Ok(Self::fixed(4)?.add(Expression::call(
            "Utf8Codec",
            Identifier::parse("maxBytes")?,
            [value].into_iter().collect::<ArgumentList>(),
        )))
    }

    fn bytes_size(&self, value: &ValueRef) -> Result<Expression> {
        Ok(Self::fixed(4)?.add(Expression::property(
            self.value(value)?,
            Identifier::parse("size")?,
        )))
    }

    fn encoded_record_size(&self, value: &ValueRef) -> Result<Expression> {
        Ok(Expression::call(
            self.value(value)?,
            Identifier::parse("wireSize")?,
            ArgumentList::default(),
        ))
    }

    fn primitive_size(primitive: Primitive) -> Result<Expression> {
        KotlinPrimitive::new(primitive)
            .wire_size()
            .map(Expression::integer)
    }

    fn enum_size(&self, id: EnumId) -> Result<Expression> {
        Enumeration::from_id(id, self.context).and_then(|enumeration| {
            KotlinPrimitive::new(enumeration.repr()?)
                .wire_size()
                .map(Expression::integer)
        })
    }

    fn unsupported(shape: &'static str) -> Result<Expression> {
        Err(Error::UnsupportedTarget {
            target: "kotlin",
            shape,
        })
    }
}

impl CodecSize for Sizer<'_> {
    type Expr = Result<Expression>;

    fn primitive(&mut self, primitive: Primitive, _value: &ValueRef) -> Self::Expr {
        Self::primitive_size(primitive)
    }

    fn string(&mut self, value: &ValueRef) -> Self::Expr {
        self.string_size(value)
    }

    fn bytes(&mut self, value: &ValueRef) -> Self::Expr {
        self.bytes_size(value)
    }

    fn direct_record(&mut self, _id: RecordId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("direct-record wire size")
    }

    fn encoded_record(&mut self, _id: RecordId, value: &ValueRef) -> Self::Expr {
        self.encoded_record_size(value)
    }

    fn c_style_enum(&mut self, id: EnumId, _value: &ValueRef) -> Self::Expr {
        self.enum_size(id)
    }

    fn data_enum(&mut self, id: EnumId, value: &ValueRef) -> Self::Expr {
        Enumeration::size_expression(id, self.value(value)?, self.context)
    }

    fn class_handle(&mut self, _id: ClassId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("class handle wire size")
    }

    fn callback_handle(&mut self, _id: CallbackId, _value: &ValueRef) -> Self::Expr {
        Self::unsupported("callback handle wire size")
    }

    fn custom(
        &mut self,
        _id: CustomTypeId,
        _value: &ValueRef,
        representation: Self::Expr,
    ) -> Self::Expr {
        representation
    }

    fn builtin(&mut self, kind: BuiltinType, value: &ValueRef) -> Self::Expr {
        match kind {
            BuiltinType::Duration | BuiltinType::SystemTime => Self::fixed(12),
            BuiltinType::Uuid => Self::fixed(16),
            BuiltinType::Url => self
                .string_expression_size(self.value(value)?.convert(Identifier::parse("toString")?)),
        }
    }

    fn optional(&mut self, value: &ValueRef, binder: BinderId, inner: Self::Expr) -> Self::Expr {
        let value = self.value(value)?;
        let binder = ValueExpression::binder(binder)?;
        Ok(value.optional_size(binder, inner?))
    }

    fn sequence(
        &mut self,
        value: &ValueRef,
        _len: &Op<ElementCount>,
        binder: BinderId,
        element: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let binder = ValueExpression::binder(binder)?;
        Ok(Self::fixed(4)?.add(value.sum_of(binder, element?)))
    }

    fn tuple(&mut self, _value: &ValueRef, _elements: Vec<Self::Expr>) -> Self::Expr {
        Self::unsupported("tuple wire size")
    }

    fn result(
        &mut self,
        value: &ValueRef,
        binder: BinderId,
        ok: Self::Expr,
        err: Self::Expr,
    ) -> Self::Expr {
        let value = self.value(value)?;
        let binder = ValueExpression::binder(binder)?;
        Ok(value.result_size(binder, ok?, err?))
    }

    fn map(
        &mut self,
        _kind: MapKind,
        _value: &ValueRef,
        _key_binder: BinderId,
        _key: Self::Expr,
        _value_binder: BinderId,
        _map_value: Self::Expr,
    ) -> Self::Expr {
        Self::unsupported("map wire size")
    }
}

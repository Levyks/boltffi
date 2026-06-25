use boltffi_binding::{
    BuiltinType, CallbackId, ClassId, CustomTypeId, DirectValueType, DirectVectorElementType,
    Direction, EnumId, HandlePresence, HandleTarget, IntoRust, Native, ParamPlanRender, Primitive,
    RecordId, Surface, TypeRef, TypeRefRender,
};

use crate::{
    bridge::jni::{DirectVectorParameter, JniType, NativeParameterKind, NativeReturn},
    core::{Error, RenderContext, Result},
    target::kotlin::{
        codec::ScalarOption,
        primitive::KotlinPrimitive,
        render::{
            class::ClassHandle, direct_vector::DirectVector, enumeration::Enumeration,
            record::Record,
        },
        syntax::TypeName,
    },
};

const KOTLIN_TARGET: &str = "kotlin";

pub struct KotlinType;

impl KotlinType {
    pub fn primitive(primitive: Primitive) -> Result<TypeName> {
        KotlinPrimitive::new(primitive).api_type()
    }

    pub fn builtin(kind: BuiltinType) -> TypeName {
        match kind {
            BuiltinType::Duration => TypeName::new("java.time.Duration"),
            BuiltinType::SystemTime => TypeName::new("java.time.Instant"),
            BuiltinType::Uuid => TypeName::new("java.util.UUID"),
            BuiltinType::Url => TypeName::new("java.net.URI"),
        }
    }

    pub fn jni(jni_type: JniType) -> Result<TypeName> {
        match jni_type {
            JniType::Boolean => Ok(TypeName::boolean()),
            JniType::Byte => Ok(TypeName::byte()),
            JniType::Short => Ok(TypeName::short()),
            JniType::Int => Ok(TypeName::int()),
            JniType::Long => Ok(TypeName::long()),
            JniType::Float => Ok(TypeName::float()),
            JniType::Double => Ok(TypeName::double()),
        }
    }

    pub fn jni_array(jni_type: JniType) -> Result<TypeName> {
        match jni_type {
            JniType::Boolean => Ok(TypeName::new("BooleanArray")),
            JniType::Byte => Ok(TypeName::new("ByteArray")),
            JniType::Short => Ok(TypeName::new("ShortArray")),
            JniType::Int => Ok(TypeName::new("IntArray")),
            JniType::Long => Ok(TypeName::new("LongArray")),
            JniType::Float => Ok(TypeName::new("FloatArray")),
            JniType::Double => Ok(TypeName::new("DoubleArray")),
        }
    }

    pub fn native_parameter(kind: &NativeParameterKind) -> Result<TypeName> {
        match kind {
            NativeParameterKind::Scalar(parameter) => Self::jni(parameter.ty()),
            NativeParameterKind::Bytes(_) | NativeParameterKind::Record(_) => {
                Ok(TypeName::byte_array(false))
            }
            NativeParameterKind::DirectVector(parameter) => Self::direct_vector(parameter),
            NativeParameterKind::Callback(_)
            | NativeParameterKind::Closure(_)
            | NativeParameterKind::Continuation(_) => Ok(TypeName::long()),
        }
    }

    pub fn native_return(return_value: &NativeReturn) -> Result<TypeName> {
        match return_value {
            NativeReturn::Void | NativeReturn::Status => Ok(TypeName::unit()),
            NativeReturn::Value(scalar) => Self::jni(scalar.jni_type()),
            NativeReturn::Bytes | NativeReturn::Record(_) => Ok(TypeName::byte_array(true)),
            NativeReturn::Callback(_) => Ok(TypeName::long()),
            NativeReturn::StatusValue(value) => Self::native_return(value.value()),
            NativeReturn::EncodedErrorValue(value) => Self::native_return(value.success().value()),
        }
    }

    pub fn type_ref(ty: &TypeRef, context: &RenderContext<Native>) -> Result<TypeName> {
        ty.render_with(&mut KotlinTypeRef::new(context))
    }

    fn direct_vector(parameter: &DirectVectorParameter) -> Result<TypeName> {
        Self::jni_array(parameter.jni_type())
    }

    pub fn direct_vector_element(element: &DirectVectorElementType) -> Result<TypeName> {
        DirectVector::from_element(element).map(|vector| vector.ty().clone())
    }
}

pub struct KotlinTypeRef<'context> {
    context: &'context RenderContext<'context, Native>,
}

impl<'context> KotlinTypeRef<'context> {
    pub fn new(context: &'context RenderContext<Native>) -> Self {
        Self { context }
    }
}

impl TypeRefRender for KotlinTypeRef<'_> {
    type Output = Result<TypeName>;

    fn primitive(&mut self, primitive: Primitive) -> Self::Output {
        KotlinType::primitive(primitive)
    }

    fn string(&mut self) -> Self::Output {
        Ok(TypeName::string())
    }

    fn bytes(&mut self) -> Self::Output {
        Ok(TypeName::byte_array(false))
    }

    fn record(&mut self, id: RecordId) -> Self::Output {
        Record::type_name_from_id(id, self.context)
    }

    fn enumeration(&mut self, id: EnumId) -> Self::Output {
        Enumeration::type_name_from_id(id, self.context)
    }

    fn class(&mut self, _id: ClassId) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "class type",
        })
    }

    fn callback(&mut self, _id: CallbackId) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "callback type",
        })
    }

    fn custom(&mut self, id: CustomTypeId) -> Self::Output {
        self.context
            .custom_type(id)
            .map(|custom_type| custom_type.representation())
            .ok_or(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "custom type without declaration",
            })?
            .render_with(self)
    }

    fn builtin(&mut self, kind: BuiltinType) -> Self::Output {
        Ok(KotlinType::builtin(kind))
    }

    fn optional(&mut self, inner: Self::Output) -> Self::Output {
        inner.map(TypeName::nullable)
    }

    fn sequence(&mut self, element: Self::Output) -> Self::Output {
        element.map(TypeName::list)
    }

    fn tuple(&mut self, _elements: Vec<Self::Output>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "tuple type",
        })
    }

    fn result(&mut self, ok: Self::Output, err: Self::Output) -> Self::Output {
        Ok(TypeName::result(ok?, err?))
    }

    fn map(&mut self, _key: Self::Output, _value: Self::Output) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: KOTLIN_TARGET,
            shape: "map type",
        })
    }
}

pub struct ParameterType<'context> {
    context: &'context RenderContext<'context, Native>,
}

impl<'context> ParameterType<'context> {
    pub fn new(context: &'context RenderContext<'context, Native>) -> Self {
        Self { context }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterType<'_> {
    type Output = Result<TypeName>;

    fn direct(
        &mut self,
        ty: &'plan DirectValueType,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match ty {
            DirectValueType::Primitive(primitive) => KotlinType::primitive(*primitive),
            DirectValueType::Record(record) => Record::type_name_from_id(*record, self.context),
            DirectValueType::Enum(enumeration) => {
                Enumeration::type_name_from_id(*enumeration, self.context)
            }
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown direct function parameter",
            }),
        }
    }

    fn encoded(
        &mut self,
        ty: &'plan TypeRef,
        _codec: &'plan <IntoRust as Direction>::Codec,
        _shape: <Native as Surface>::BufferShape,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        KotlinType::type_ref(ty, self.context)
    }

    fn handle(
        &mut self,
        target: &'plan HandleTarget,
        _carrier: <Native as Surface>::HandleCarrier,
        presence: HandlePresence,
        _receive: <IntoRust as Direction>::Receive,
    ) -> Self::Output {
        match target {
            HandleTarget::Class(class) => {
                ClassHandle::new(*class, presence, self.context).and_then(|handle| handle.ty())
            }
            HandleTarget::Callback(_) | HandleTarget::Stream(_) => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "handle function parameter",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: KOTLIN_TARGET,
                shape: "unknown handle function parameter",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        ScalarOption::new(primitive).ty()
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        KotlinType::direct_vector_element(element)
    }
}

use boltffi_binding::{
    BuiltinType, ClosureReturn, HandlePresence, HandleTarget, IntoRust, Native, OutOfRust,
    ParamPlan, ParamPlanRender, Primitive, ReadPlan, Receive, ReturnPlan, ReturnPlanRender,
    ReturnValueSlot, TypeRef, WritePlan, native,
};

use crate::{
    core::{Error, Result},
    target::python::render::Package,
};

pub struct TypeHint {
    annotation: String,
    uses_sequence: bool,
}

impl TypeHint {
    pub fn from_type_ref(ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        match ty {
            TypeRef::Primitive(primitive) => Self::from_primitive(*primitive),
            TypeRef::String => Ok(Self::new("str")),
            TypeRef::Bytes => Ok(Self::new("bytes")),
            TypeRef::Builtin(builtin) => Ok(Self::from_builtin(*builtin)),
            TypeRef::Custom(custom_type) => {
                Self::from_type_ref(package.custom_representation(*custom_type)?, package)
            }
            TypeRef::Optional(inner) => Ok(Self::new(format!(
                "{} | None",
                Self::from_type_ref(inner, package)?.into_string()
            ))),
            TypeRef::Result { ok, err } => Ok(Self::new(format!(
                "tuple[bool, {} | {}]",
                Self::from_type_ref(ok, package)?.into_string(),
                Self::from_type_ref(err, package)?.into_string()
            ))),
            TypeRef::Sequence(element) => Ok(Self::new(format!(
                "list[{}]",
                Self::from_type_ref(element, package)?.into_string()
            ))),
            TypeRef::Tuple(elements) => Ok(Self::new(format!(
                "tuple[{}]",
                elements
                    .iter()
                    .map(|element| Self::from_type_ref(element, package).map(Self::into_string))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ")
            ))),
            TypeRef::Map { key, value } => Ok(Self::new(format!(
                "dict[{}, {}]",
                Self::from_type_ref(key, package)?.into_string(),
                Self::from_type_ref(value, package)?.into_string()
            ))),
            TypeRef::Record(record) => Ok(Self::new(package.record_name(*record)?)),
            TypeRef::Enum(enumeration) => Ok(Self::new(package.enum_name(*enumeration)?)),
            TypeRef::Class(class) => Ok(Self::new(package.class_name(class)?)),
            TypeRef::Callback(_) => Ok(Self::new("object")),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported type annotation",
            }),
        }
    }

    pub fn from_parameter(
        plan: &ParamPlan<Native, IntoRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        plan.render_with(&mut ParameterHint { package })
    }

    pub fn from_return(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        plan.render_with(&mut ReturnHint { package })
    }

    pub fn from_primitive(primitive: Primitive) -> Result<Self> {
        Ok(match primitive {
            Primitive::Bool => Self::new("bool"),
            Primitive::F32 | Primitive::F64 => Self::new("float"),
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize => Self::new("int"),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unsupported primitive type hint",
                });
            }
        })
    }

    pub fn into_string(self) -> String {
        self.annotation
    }

    pub fn uses_sequence(&self) -> bool {
        self.uses_sequence
    }

    fn from_parameter_type_ref(ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        match ty {
            TypeRef::Custom(custom_type) => {
                Self::from_parameter_type_ref(package.custom_representation(*custom_type)?, package)
            }
            TypeRef::Optional(inner) => {
                let inner = Self::from_parameter_type_ref(inner, package)?;
                Ok(Self::compose(
                    format!("{} | None", inner.annotation),
                    [inner],
                ))
            }
            TypeRef::Result { ok, err } => {
                let ok = Self::from_parameter_type_ref(ok, package)?;
                let err = Self::from_parameter_type_ref(err, package)?;
                Ok(Self::compose(
                    format!("tuple[bool, {} | {}]", ok.annotation, err.annotation),
                    [ok, err],
                ))
            }
            TypeRef::Sequence(element) => {
                let element = Self::from_parameter_type_ref(element, package)?;
                Ok(Self::sequence(format!(
                    "Sequence[{}]",
                    element.into_string()
                )))
            }
            TypeRef::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| Self::from_parameter_type_ref(element, package))
                    .collect::<Result<Vec<_>>>()?;
                let annotation = format!(
                    "tuple[{}]",
                    elements
                        .iter()
                        .map(|element| element.annotation.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                Ok(Self::compose(annotation, elements))
            }
            TypeRef::Map { key, value } => {
                let key = Self::from_parameter_type_ref(key, package)?;
                let value = Self::from_parameter_type_ref(value, package)?;
                Ok(Self::compose(
                    format!("dict[{}, {}]", key.annotation, value.annotation),
                    [key, value],
                ))
            }
            _ => Self::from_type_ref(ty, package),
        }
    }

    fn from_direct_vector_parameter(element: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        if matches!(element, TypeRef::Primitive(Primitive::U8)) {
            return Ok(Self::sequence("bytes | Sequence[int]"));
        }
        let element = Self::from_type_ref(element, package)?;
        Ok(Self::sequence(format!("Sequence[{}]", element.annotation)))
    }

    fn new(annotation: impl Into<String>) -> Self {
        Self {
            annotation: annotation.into(),
            uses_sequence: false,
        }
    }

    fn sequence(annotation: impl Into<String>) -> Self {
        Self {
            annotation: annotation.into(),
            uses_sequence: true,
        }
    }

    fn compose(annotation: impl Into<String>, parts: impl IntoIterator<Item = Self>) -> Self {
        Self {
            annotation: annotation.into(),
            uses_sequence: parts.into_iter().any(|part| part.uses_sequence),
        }
    }

    fn from_builtin(builtin: BuiltinType) -> Self {
        match builtin {
            BuiltinType::Duration | BuiltinType::SystemTime => Self::new("float"),
            BuiltinType::Uuid | BuiltinType::Url => Self::new("str"),
        }
    }
}

struct ParameterHint<'package, 'binding, 'bridge> {
    package: &'package Package<'binding, 'bridge>,
}

impl ParameterHint<'_, '_, '_> {
    fn direct_type_ref(&self, ty: &TypeRef) -> Result<TypeHint> {
        match ty {
            TypeRef::Primitive(primitive) => TypeHint::from_primitive(*primitive),
            TypeRef::Record(record) => Ok(TypeHint::new(self.package.record_name(*record)?)),
            TypeRef::Enum(enumeration) => Ok(TypeHint::new(self.package.enum_name(*enumeration)?)),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter stub",
            }),
        }
    }

    fn encoded_type_ref(&self, ty: &TypeRef, shape: native::BufferShape) -> Result<TypeHint> {
        if shape != native::BufferShape::Slice {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter stub",
            });
        }
        match ty {
            TypeRef::String => Ok(TypeHint::new("str")),
            TypeRef::Bytes => Ok(TypeHint::new("bytes")),
            TypeRef::Custom(custom_type) => TypeHint::from_parameter_type_ref(
                self.package.custom_representation(*custom_type)?,
                self.package,
            ),
            TypeRef::Record(record) => Ok(TypeHint::new(self.package.record_name(*record)?)),
            TypeRef::Enum(enumeration) => Ok(TypeHint::new(self.package.enum_name(*enumeration)?)),
            _ => TypeHint::from_parameter_type_ref(ty, self.package),
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterHint<'_, '_, '_> {
    type Output = Result<TypeHint>;

    fn direct(&mut self, ty: &TypeRef, _: Receive) -> Self::Output {
        self.direct_type_ref(ty)
    }

    fn encoded(
        &mut self,
        ty: &TypeRef,
        _: &WritePlan,
        shape: native::BufferShape,
        _: Receive,
    ) -> Self::Output {
        self.encoded_type_ref(ty, shape)
    }

    fn handle(
        &mut self,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
        _: Receive,
    ) -> Self::Output {
        match (target, presence) {
            (HandleTarget::Class(class_id), HandlePresence::Required) => {
                Ok(TypeHint::new(self.package.class_name(class_id)?))
            }
            (HandleTarget::Class(class_id), HandlePresence::Nullable) => Ok(TypeHint::new(
                format!("{} | None", self.package.class_name(class_id)?),
            )),
            (HandleTarget::Callback(_), _) => Ok(TypeHint::new("object")),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter stub",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(TypeHint::new(format!(
            "{} | None",
            TypeHint::from_primitive(primitive)?.into_string()
        )))
    }

    fn direct_vector(&mut self, element: &TypeRef) -> Self::Output {
        TypeHint::from_direct_vector_parameter(element, self.package)
    }
}

struct ReturnHint<'package, 'binding, 'bridge> {
    package: &'package Package<'binding, 'bridge>,
}

impl ReturnHint<'_, '_, '_> {
    fn type_ref(&self, ty: &TypeRef) -> Result<TypeHint> {
        TypeHint::from_type_ref(ty, self.package)
    }
}

impl<'plan> ReturnPlanRender<'plan, Native, OutOfRust> for ReturnHint<'_, '_, '_> {
    type Output = Result<TypeHint>;

    fn void(&mut self) -> Self::Output {
        Ok(TypeHint::new("None"))
    }

    fn direct(&mut self, _: ReturnValueSlot, ty: &TypeRef) -> Self::Output {
        self.type_ref(ty)
    }

    fn encoded(
        &mut self,
        _: ReturnValueSlot,
        ty: &TypeRef,
        _: &ReadPlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Buffer => self.type_ref(ty),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported return stub",
            }),
        }
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        target: &HandleTarget,
        _: native::HandleCarrier,
        presence: HandlePresence,
    ) -> Self::Output {
        match (target, presence) {
            (HandleTarget::Class(class_id), HandlePresence::Required) => {
                Ok(TypeHint::new(self.package.class_name(class_id)?))
            }
            (HandleTarget::Callback(_), _) => Ok(TypeHint::new("object")),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported return stub",
            }),
        }
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Ok(TypeHint::new(format!(
            "{} | None",
            TypeHint::from_primitive(primitive)?.into_string()
        )))
    }

    fn direct_vector(&mut self, element: &TypeRef) -> Self::Output {
        Ok(TypeHint::new(format!(
            "list[{}]",
            self.type_ref(element)?.into_string()
        )))
    }

    fn closure(&mut self, _: &ClosureReturn<Native, OutOfRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported return stub",
        })
    }
}

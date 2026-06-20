use boltffi_binding::{
    ClosureReturn, DirectValueType, DirectVectorElementType, ErrorDecl, HandlePresence,
    HandleTarget, IntoRust, Native, Primitive, ReturnPlan, ReturnPlanRender, ReturnValueSlot,
    TypeRef, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Error, RenderContext, Result},
    target::python::{
        codec::OwnedPayload,
        cpython::render::{direct, direct_vector, primitive},
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallibleReturn {
    pub declarations: Vec<c::Statement>,
    pub success: FallibleSuccess,
    pub error: FallibleError,
}

impl FallibleReturn {
    pub fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        error: &ErrorDecl<Native, IntoRust>,
        return_out: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let ErrorDecl::EncodedViaReturnSlot { codec: error, .. } = error else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "foreign callable error channel",
            });
        };
        let success = FallibleSuccess::new(plan, return_out, bridge, context)?;
        let error = FallibleError::new(error)?;
        let declarations = return_out
            .map(|ty| TypeFragment::declaration(ty, "return_out"))
            .transpose()?
            .into_iter()
            .collect();
        Ok(Self {
            declarations,
            success,
            error,
        })
    }

    pub fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.success.primitive().into_iter()
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.success
            .wire_primitive()
            .into_iter()
            .chain(self.error.wire_primitive())
    }

    pub fn direct_vectors(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.success
            .direct_vector()
            .into_iter()
            .chain(self.error.direct_vector())
    }

    pub fn uses_wire_payload(&self) -> bool {
        true
    }

    pub fn has_string(&self) -> bool {
        self.success.string || self.error.string
    }

    pub fn has_bytes(&self) -> bool {
        self.success.bytes || self.error.bytes
    }

    pub fn has_raw_wire(&self) -> bool {
        self.success.raw_wire || self.error.raw_wire
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallibleSuccess {
    out: Option<Identifier>,
    value: Option<Identifier>,
    c_type: Option<c::TypeFragment>,
    default_value: Option<c::Expression>,
    parser: Option<Identifier>,
    pub wire: bool,
    pub direct: bool,
    void: bool,
    primitive: Option<primitive::Runtime>,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    string: bool,
    bytes: bool,
    raw_wire: bool,
}

impl FallibleSuccess {
    fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        return_out: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut FallibleSuccessValue {
            return_out: return_out.cloned(),
            bridge,
            context,
        })
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    fn wire_primitive(&self) -> Option<primitive::Runtime> {
        self.wire_primitive
    }

    fn direct_vector(&self) -> Option<direct_vector::Element> {
        self.direct_vector.clone()
    }

    fn void() -> Self {
        Self {
            out: None,
            value: None,
            c_type: None,
            default_value: None,
            parser: None,
            wire: false,
            direct: false,
            void: true,
            primitive: None,
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        }
    }

    fn direct(
        ty: &DirectValueType,
        out_type: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        Ok(Self {
            out: Some(Identifier::parse("return_out")?),
            value: Some(Identifier::parse("return_success")?),
            c_type: Some(TypeFragment::anonymous(out_type)?),
            default_value: Some(direct.default_value().clone()),
            parser: Some(direct.parser().clone()),
            wire: false,
            direct: true,
            void: false,
            primitive: direct.primitive(),
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        })
    }

    fn wire(codec: &WritePlan, out_type: &c::Type) -> Result<Self> {
        if !matches!(out_type, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible foreign encoded out-parameter",
            });
        }
        let encoded = OwnedPayload::write(codec)?;
        Ok(Self {
            out: Some(Identifier::parse("return_out")?),
            value: Some(Identifier::parse("return_success")?),
            c_type: None,
            default_value: None,
            parser: Some(encoded.parser().clone()),
            wire: true,
            direct: false,
            void: false,
            primitive: None,
            wire_primitive: encoded.primitive(),
            direct_vector: encoded.direct_vector(),
            string: encoded.has_string(),
            bytes: encoded.has_bytes(),
            raw_wire: encoded.has_raw_wire(),
        })
    }

    fn out_type(return_out: Option<&c::Type>) -> Result<&c::Type> {
        match return_out {
            Some(c::Type::MutPointer(ty)) => Ok(ty.as_ref()),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible foreign success out-parameter",
            }),
        }
    }

    pub fn out(&self) -> &Identifier {
        self.out
            .as_ref()
            .expect("fallible foreign success has an out parameter")
    }

    pub fn value(&self) -> &Identifier {
        self.value
            .as_ref()
            .expect("fallible foreign success has a value binding")
    }

    pub fn c_type(&self) -> &c::TypeFragment {
        self.c_type
            .as_ref()
            .expect("direct fallible foreign success has a C type")
    }

    pub fn default_value(&self) -> &c::Expression {
        self.default_value
            .as_ref()
            .expect("direct fallible foreign success has a default value")
    }

    pub fn parser(&self) -> &Identifier {
        self.parser
            .as_ref()
            .expect("fallible foreign success has a parser")
    }
}

struct FallibleSuccessValue<'render> {
    return_out: Option<c::Type>,
    bridge: &'render PythonCExtBridgeContract,
    context: &'render RenderContext<'render, Native>,
}

impl<'plan, 'render> ReturnPlanRender<'plan, Native, IntoRust> for FallibleSuccessValue<'render> {
    type Output = Result<FallibleSuccess>;

    fn void(&mut self) -> Self::Output {
        if self.return_out.is_some() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "void foreign success out-parameter",
            });
        }
        Ok(FallibleSuccess::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => FallibleSuccess::direct(
                ty,
                FallibleSuccess::out_type(self.return_out.as_ref())?,
                self.bridge,
                self.context,
            ),
            ReturnValueSlot::ReturnSlot => Self::unsupported(),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible foreign success slot",
            }),
        }
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        codec: &'plan WritePlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::OutPointer, native::BufferShape::Buffer) => {
                FallibleSuccess::wire(codec, FallibleSuccess::out_type(self.return_out.as_ref())?)
            }
            (ReturnValueSlot::OutPointer, _) | (ReturnValueSlot::ReturnSlot, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible foreign success slot",
            }),
        }
    }

    fn handle(
        &mut self,
        _: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        Self::unsupported()
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Self::unsupported()
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Self::unsupported()
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl<'render> FallibleSuccessValue<'render> {
    fn unsupported() -> Result<FallibleSuccess> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported fallible foreign success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FallibleError {
    pub value: Identifier,
    pub parser: Identifier,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    string: bool,
    bytes: bool,
    raw_wire: bool,
}

impl FallibleError {
    fn new(codec: &WritePlan) -> Result<Self> {
        let encoded = OwnedPayload::write(codec)?;
        Ok(Self {
            value: Identifier::parse("return_value")?,
            parser: encoded.parser().clone(),
            wire_primitive: encoded.primitive(),
            direct_vector: encoded.direct_vector(),
            string: encoded.has_string(),
            bytes: encoded.has_bytes(),
            raw_wire: encoded.has_raw_wire(),
        })
    }

    fn wire_primitive(&self) -> Option<primitive::Runtime> {
        self.wire_primitive
    }

    fn direct_vector(&self) -> Option<direct_vector::Element> {
        self.direct_vector.clone()
    }
}

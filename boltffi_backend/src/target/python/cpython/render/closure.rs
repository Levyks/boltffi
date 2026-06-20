use askama::Template as AskamaTemplate;
use boltffi_binding::{
    ClosureParameter, ClosureReturn, DirectValueType, DirectVectorElementType, ErrorDecl,
    HandlePresence, HandleTarget, IntoRust, Native, OutOfRust, OutgoingParam, ParamDecl,
    ParamPlanRender, Primitive, ReadPlan, ReturnPlan, ReturnPlanRender, ReturnValueSlot, TypeRef,
    WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Expression, Identifier, Literal, TypeFragment, syntax::TypeSyntax},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Error, RenderContext, Result},
    target::python::{
        codec::{BorrowedPayload, OwnedPayload},
        cpython::render::{direct, direct_vector, primitive},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/closure.c", escape = "none")]
struct Template {
    invoke: Identifier,
    release: Identifier,
    parser: Identifier,
    call_output_declaration: c::Statement,
    context_output_declaration: c::Statement,
    release_output_declaration: c::Statement,
    copy_buffer_storage: Identifier,
    params: Vec<Argument>,
    returns: ReturnValue,
    fallible_return: Option<FallibleReturn>,
    wire_payload: bool,
}

pub struct Parameter {
    call_declaration: c::Statement,
    call: Identifier,
    context_declaration: c::Statement,
    context: Identifier,
    release_declaration: c::Statement,
    release: Identifier,
    parser: Identifier,
    release_needed: Identifier,
    source: String,
    primitives: Vec<primitive::Runtime>,
    wire_primitives: Vec<primitive::Runtime>,
    direct_vectors: Vec<direct_vector::Element>,
    string_argument: bool,
    bytes_argument: bool,
    raw_wire_argument: bool,
}

impl Parameter {
    pub fn new(
        owner: &str,
        index: usize,
        name: Identifier,
        parameter: &ClosureParameter<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let signature = Signature::new(parameter, c_parameters, bridge, context)?;
        let prefix = Identifier::escape(format!("{owner}_{index}_{name}"))?;
        let invoke = Identifier::parse(format!("boltffi_python_closure_{prefix}_invoke"))?;
        let release = Identifier::parse(format!("boltffi_python_closure_{prefix}_release"))?;
        let parser = Identifier::parse(format!("boltffi_python_parse_closure_{prefix}"))?;
        let call = Identifier::parse(c_parameters[0].name())?;
        let context_name = Identifier::parse(c_parameters[1].name())?;
        let release_name = Identifier::parse(c_parameters[2].name())?;
        let release_needed = Identifier::escape(format!("{name}_release_needed"))?;
        let copy_buffer_storage = Self::copy_buffer_storage(bridge)?;
        let source = Template {
            invoke,
            release,
            parser: parser.clone(),
            call_output_declaration: OutputParameter::new(c_parameters[0].ty(), "out_call")
                .declaration()?,
            context_output_declaration: OutputParameter::new(c_parameters[1].ty(), "out_context")
                .declaration()?,
            release_output_declaration: OutputParameter::new(c_parameters[2].ty(), "out_release")
                .declaration()?,
            copy_buffer_storage,
            wire_payload: signature.wire_payload(),
            params: signature.params.clone(),
            returns: signature.returns.clone(),
            fallible_return: signature.fallible_return.clone(),
        }
        .render()?;
        Ok(Self {
            call_declaration: TypeSyntax::new(c_parameters[0].ty()).declaration(call.as_str())?,
            call,
            context_declaration: TypeSyntax::new(c_parameters[1].ty())
                .declaration(context_name.as_str())?,
            context: context_name,
            release_declaration: TypeSyntax::new(c_parameters[2].ty())
                .declaration(release_name.as_str())?,
            release: release_name,
            parser,
            release_needed,
            source,
            primitives: signature.primitives(),
            wire_primitives: signature.wire_primitives(),
            direct_vectors: signature.direct_vectors(),
            string_argument: signature.has_string_argument(),
            bytes_argument: signature.has_bytes_argument(),
            raw_wire_argument: signature.has_raw_wire_argument(),
        })
    }

    pub fn c_arity() -> usize {
        3
    }

    pub fn call_args(&self) -> [Identifier; 3] {
        [
            self.call.clone(),
            self.context.clone(),
            self.release.clone(),
        ]
    }

    pub fn call_declaration(&self) -> &c::Statement {
        &self.call_declaration
    }

    pub fn context_declaration(&self) -> &c::Statement {
        &self.context_declaration
    }

    pub fn release_declaration(&self) -> &c::Statement {
        &self.release_declaration
    }

    pub fn declaration(&self) -> &str {
        &self.source
    }

    pub fn parser(&self) -> &Identifier {
        &self.parser
    }

    pub fn call(&self) -> &Identifier {
        &self.call
    }

    pub fn context(&self) -> &Identifier {
        &self.context
    }

    pub fn release(&self) -> &Identifier {
        &self.release
    }

    pub fn release_needed(&self) -> &Identifier {
        &self.release_needed
    }

    pub fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.primitives.iter().copied()
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.wire_primitives.iter().copied()
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.direct_vectors.iter().cloned()
    }

    pub fn has_string_argument(&self) -> bool {
        self.string_argument
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.bytes_argument
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.raw_wire_argument
    }

    fn copy_buffer_storage(bridge: &PythonCExtBridgeContract) -> Result<Identifier> {
        Ok(bridge.buffer_from_bytes()?.storage_name().clone())
    }
}

struct OutputParameter<'ty> {
    ty: &'ty c::Type,
    name: &'static str,
}

impl<'ty> OutputParameter<'ty> {
    fn new(ty: &'ty c::Type, name: &'static str) -> Self {
        Self { ty, name }
    }

    fn declaration(&self) -> Result<c::Statement> {
        match self.ty {
            c::Type::FunctionPointer { returns, params } => {
                TypeSyntax::function_pointer_declaration(
                    format!("*{}", self.name).as_str(),
                    returns,
                    params.iter(),
                )
            }
            _ => Ok(c::Statement::new(format!(
                "{} *{}",
                TypeSyntax::new(self.ty).anonymous()?,
                self.name
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Signature {
    params: Vec<Argument>,
    returns: ReturnValue,
    fallible_return: Option<FallibleReturn>,
}

impl Signature {
    fn new(
        parameter: &ClosureParameter<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [call, context_param, release_param, ..] = c_parameters else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure parameter ABI",
            });
        };
        Self::validate_context(context_param.ty())?;
        Self::validate_release(release_param.ty())?;
        let c::Type::FunctionPointer {
            returns,
            params: call_params,
        } = call.ty()
        else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure call ABI",
            });
        };
        let invoke = parameter.invoke();
        let arity = invoke
            .params()
            .iter()
            .map(Argument::arity)
            .collect::<Result<Vec<_>>>()?;
        let return_out_count = Self::return_out_count(invoke.returns().plan())?;
        let value_count = arity.iter().sum::<usize>();
        let value_start = 1;
        let value_end = value_start + value_count;
        if call_params.len() != value_end + return_out_count {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure invoke ABI",
            });
        }
        let param_types = arity
            .iter()
            .scan(value_start, |offset, count| {
                let start = *offset;
                *offset += *count;
                Some(&call_params[start..*offset])
            })
            .collect::<Vec<_>>();
        let params = invoke
            .params()
            .iter()
            .zip(param_types)
            .map(|(parameter, c_types)| Argument::new(parameter, c_types, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let return_out = (return_out_count == 1).then(|| &call_params[value_end]);
        let fallible_return = match invoke.error() {
            ErrorDecl::None(_) => None,
            ErrorDecl::EncodedViaReturnSlot { .. } => Some(FallibleReturn::new(
                invoke.returns().plan(),
                invoke.error(),
                return_out,
                bridge,
                context,
            )?),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "closure error channel",
                });
            }
        };
        let returns = match &fallible_return {
            Some(_) => ReturnValue::fallible_error(returns)?,
            None => ReturnValue::new(invoke.returns().plan(), returns, bridge, context)?,
        };
        Ok(Self {
            params,
            returns,
            fallible_return,
        })
    }

    fn primitives(&self) -> Vec<primitive::Runtime> {
        self.params
            .iter()
            .filter_map(Argument::primitive)
            .chain(self.returns.primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::primitives),
            )
            .collect()
    }

    fn wire_primitives(&self) -> Vec<primitive::Runtime> {
        self.params
            .iter()
            .filter_map(Argument::wire_primitive)
            .chain(self.returns.wire_primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::wire_primitives),
            )
            .collect()
    }

    fn direct_vectors(&self) -> Vec<direct_vector::Element> {
        self.params
            .iter()
            .filter_map(Argument::direct_vector_element)
            .chain(self.returns.direct_vector())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::direct_vectors),
            )
            .collect()
    }

    fn wire_payload(&self) -> bool {
        self.returns.wire
            || self
                .fallible_return
                .as_ref()
                .is_some_and(FallibleReturn::uses_wire_payload)
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(Argument::has_string)
            || self.returns.has_string()
            || self.fallible_return.iter().any(FallibleReturn::has_string)
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(Argument::has_bytes)
            || self.returns.has_bytes()
            || self.fallible_return.iter().any(FallibleReturn::has_bytes)
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(Argument::has_raw_wire)
            || self.returns.has_raw_wire()
            || self
                .fallible_return
                .iter()
                .any(FallibleReturn::has_raw_wire)
    }

    fn validate_context(ty: &c::Type) -> Result<()> {
        match ty {
            c::Type::MutPointer(inner) if matches!(inner.as_ref(), c::Type::Void) => Ok(()),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure context ABI",
            }),
        }
    }

    fn validate_release(ty: &c::Type) -> Result<()> {
        match ty {
            c::Type::FunctionPointer { returns, params }
                if matches!(returns.as_ref(), c::Type::Void)
                    && matches!(
                        params.as_slice(),
                        [c::Type::MutPointer(inner)] if matches!(inner.as_ref(), c::Type::Void)
                    ) =>
            {
                Ok(())
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure release ABI",
            }),
        }
    }

    fn return_out_count(plan: &ReturnPlan<Native, IntoRust>) -> Result<usize> {
        plan.render_with(&mut ClosureReturnOutCount)
    }
}

struct ClosureReturnOutCount;

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ClosureReturnOutCount {
    type Output = Result<usize>;

    fn void(&mut self) -> Self::Output {
        Ok(0)
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &'plan DirectValueType) -> Self::Output {
        Self::slot_count(slot)
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        _: &'plan WritePlan,
        _: native::BufferShape,
    ) -> Self::Output {
        Self::slot_count(slot)
    }

    fn handle(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
    ) -> Self::Output {
        Self::slot_count(slot)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Ok(0)
    }

    fn direct_vector(&mut self, _: &'plan DirectVectorElementType) -> Self::Output {
        Ok(0)
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "closure return from closure parameter",
        })
    }
}

impl ClosureReturnOutCount {
    fn slot_count(slot: ReturnValueSlot) -> Result<usize> {
        match slot {
            ReturnValueSlot::ReturnSlot => Ok(0),
            ReturnValueSlot::OutPointer => Ok(1),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown closure return plan",
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Argument {
    declarations: Vec<c::Statement>,
    object: Identifier,
    expression: c::Expression,
    primitive: Option<primitive::Runtime>,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    string: bool,
    bytes: bool,
    raw_wire: bool,
}

impl Argument {
    fn arity(parameter: &ParamDecl<Native, OutOfRust>) -> Result<usize> {
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut ClosureArgumentArity),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown closure argument",
            }),
        }
    }

    fn new(
        parameter: &ParamDecl<Native, OutOfRust>,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Identifier::escape(Name::new(parameter.name()).function_text()?)?;
        let object = Identifier::parse(format!("{name}_object"))?;
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut ClosureArgumentValue {
                name,
                object,
                c_types,
                bridge,
                context,
            }),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported closure argument",
            }),
        }
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    fn wire_primitive(&self) -> Option<primitive::Runtime> {
        self.wire_primitive
    }

    fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        self.direct_vector.clone()
    }

    fn has_string(&self) -> bool {
        self.string
    }

    fn has_bytes(&self) -> bool {
        self.bytes
    }

    fn has_raw_wire(&self) -> bool {
        self.raw_wire
    }

    fn direct(
        name: Identifier,
        object: Identifier,
        ty: &DirectValueType,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [c_type] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure direct argument ABI",
            });
        };
        let direct = direct::NativeSlot::from_direct_value(ty, bridge, context)?;
        Ok(Self {
            declarations: vec![TypeSyntax::new(c_type).declaration(name.as_str())?],
            object,
            expression: direct.box_expression(name),
            primitive: direct.primitive(),
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        })
    }

    fn encoded(
        name: Identifier,
        object: Identifier,
        codec: &ReadPlan,
        c_types: &[c::Type],
    ) -> Result<Self> {
        let [pointer, length] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure encoded argument ABI",
            });
        };
        let pointer_name = Identifier::parse(format!("{name}_ptr"))?;
        let length_name = Identifier::parse(format!("{name}_len"))?;
        let payload = BorrowedPayload::read(codec, pointer_name.clone(), length_name.clone())?;
        let wire_primitive = payload.primitive();
        let string = payload.has_string();
        let bytes = payload.has_bytes();
        let raw_wire = payload.has_raw_wire();
        let expression = payload.expression();
        Ok(Self {
            declarations: vec![
                TypeSyntax::new(pointer).declaration(pointer_name.as_str())?,
                TypeSyntax::new(length).declaration(length_name.as_str())?,
            ],
            object,
            expression,
            primitive: None,
            wire_primitive,
            direct_vector: None,
            string,
            bytes,
            raw_wire,
        })
    }

    fn direct_vector(
        name: Identifier,
        object: Identifier,
        element: &DirectVectorElementType,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [pointer, length] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure vector argument ABI",
            });
        };
        let pointer_name = Identifier::parse(format!("{name}_ptr"))?;
        let length_name = Identifier::parse(format!("{name}_len"))?;
        let element = direct_vector::Element::from_element(element, bridge, context)?;
        Ok(Self {
            declarations: vec![
                TypeSyntax::new(pointer).declaration(pointer_name.as_str())?,
                TypeSyntax::new(length).declaration(length_name.as_str())?,
            ],
            object,
            expression: c::Expression::call(
                element.vector_boxer().clone(),
                c::ArgumentList::from_iter([
                    c::Expression::identifier(pointer_name),
                    c::Expression::identifier(length_name),
                ]),
            ),
            primitive: None,
            wire_primitive: None,
            direct_vector: Some(element),
            string: false,
            bytes: false,
            raw_wire: false,
        })
    }
}

struct ClosureArgumentArity;

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ClosureArgumentArity {
    type Output = Result<usize>;

    fn direct(&mut self, _: &DirectValueType, _: ()) -> Self::Output {
        Ok(1)
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        _: &ReadPlan,
        shape: native::BufferShape,
        _: (),
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => Ok(2),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported closure argument",
            }),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn direct_vector(&mut self, _: &DirectVectorElementType) -> Self::Output {
        Ok(2)
    }
}

struct ClosureArgumentValue<'ctype, 'bridge, 'context, 'bindings> {
    name: Identifier,
    object: Identifier,
    c_types: &'ctype [c::Type],
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for ClosureArgumentValue<'_, '_, '_, '_> {
    type Output = Result<Argument>;

    fn direct(&mut self, ty: &DirectValueType, _: ()) -> Self::Output {
        Argument::direct(
            self.name.clone(),
            self.object.clone(),
            ty,
            self.c_types,
            self.bridge,
            self.context,
        )
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        codec: &ReadPlan,
        shape: native::BufferShape,
        _: (),
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => {
                Argument::encoded(self.name.clone(), self.object.clone(), codec, self.c_types)
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported closure argument",
            }),
        }
    }

    fn handle(
        &mut self,
        _: &HandleTarget,
        _: native::HandleCarrier,
        _: HandlePresence,
        _: (),
    ) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure argument",
        })
    }

    fn direct_vector(&mut self, element: &DirectVectorElementType) -> Self::Output {
        Argument::direct_vector(
            self.name.clone(),
            self.object.clone(),
            element,
            self.c_types,
            self.bridge,
            self.context,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleReturn {
    declarations: Vec<c::Statement>,
    success: FallibleSuccess,
    error: FallibleError,
}

impl FallibleReturn {
    fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        error: &ErrorDecl<Native, IntoRust>,
        return_out: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let ErrorDecl::EncodedViaReturnSlot { codec: error, .. } = error else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure error channel",
            });
        };
        let success = FallibleSuccess::new(plan, return_out, bridge, context)?;
        let error = FallibleError::new(error)?;
        let declarations = return_out
            .map(|ty| TypeSyntax::new(ty).declaration("return_out"))
            .transpose()?
            .into_iter()
            .collect();
        Ok(Self {
            declarations,
            success,
            error,
        })
    }

    fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.success.primitive().into_iter()
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.success
            .wire_primitive()
            .into_iter()
            .chain(self.error.wire_primitive())
    }

    fn direct_vectors(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.success
            .direct_vector()
            .into_iter()
            .chain(self.error.direct_vector())
    }

    fn uses_wire_payload(&self) -> bool {
        true
    }

    fn has_string(&self) -> bool {
        self.success.string || self.error.string
    }

    fn has_bytes(&self) -> bool {
        self.success.bytes || self.error.bytes
    }

    fn has_raw_wire(&self) -> bool {
        self.success.raw_wire || self.error.raw_wire
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleSuccess {
    out: Option<Identifier>,
    value: Option<Identifier>,
    c_type: Option<c::TypeFragment>,
    default_value: Option<c::Expression>,
    parser: Option<Identifier>,
    wire: bool,
    direct: bool,
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
            return_out,
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
            c_type: Some(TypeSyntax::new(out_type).anonymous()?),
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
                shape: "fallible closure encoded out-parameter",
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
                shape: "fallible closure success out-parameter",
            }),
        }
    }

    fn out(&self) -> &Identifier {
        self.out
            .as_ref()
            .expect("fallible closure success has an out parameter")
    }

    fn value(&self) -> &Identifier {
        self.value
            .as_ref()
            .expect("fallible closure success has a value binding")
    }

    fn c_type(&self) -> &c::TypeFragment {
        self.c_type
            .as_ref()
            .expect("direct fallible closure success has a C type")
    }

    fn default_value(&self) -> &c::Expression {
        self.default_value
            .as_ref()
            .expect("direct fallible closure success has a default value")
    }

    fn parser(&self) -> &Identifier {
        self.parser
            .as_ref()
            .expect("fallible closure success has a parser")
    }
}

struct FallibleSuccessValue<'out, 'bridge, 'context, 'bindings> {
    return_out: Option<&'out c::Type>,
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for FallibleSuccessValue<'_, '_, '_, '_> {
    type Output = Result<FallibleSuccess>;

    fn void(&mut self) -> Self::Output {
        if self.return_out.is_some() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "void closure success out-parameter",
            });
        }
        Ok(FallibleSuccess::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => FallibleSuccess::direct(
                ty,
                FallibleSuccess::out_type(self.return_out)?,
                self.bridge,
                self.context,
            ),
            ReturnValueSlot::ReturnSlot => Self::unsupported(),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible closure success slot",
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
                FallibleSuccess::wire(codec, FallibleSuccess::out_type(self.return_out)?)
            }
            (ReturnValueSlot::OutPointer, _) | (ReturnValueSlot::ReturnSlot, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible closure success slot",
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

impl FallibleSuccessValue<'_, '_, '_, '_> {
    fn unsupported() -> Result<FallibleSuccess> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported fallible closure success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleError {
    value: Identifier,
    parser: Identifier,
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReturnValue {
    c_type: TypeFragment,
    parser: Option<Identifier>,
    default_value: Expression,
    value: Option<Identifier>,
    primitive: Option<primitive::Runtime>,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    wire: bool,
    string: bool,
    bytes: bool,
    raw_wire: bool,
    void: bool,
}

impl ReturnValue {
    fn fallible_error(c_type: &c::Type) -> Result<Self> {
        Ok(Self {
            c_type: TypeSyntax::new(c_type).anonymous()?,
            parser: None,
            default_value: Expression::literal(Literal::compound_zero()),
            value: Some(Identifier::parse("return_value")?),
            primitive: None,
            wire_primitive: None,
            direct_vector: None,
            wire: true,
            string: false,
            bytes: false,
            raw_wire: false,
            void: false,
        })
    }

    fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        c_type: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut ClosureReturnValue {
            c_type,
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

    fn has_string(&self) -> bool {
        self.string
    }

    fn has_bytes(&self) -> bool {
        self.bytes
    }

    fn has_raw_wire(&self) -> bool {
        self.raw_wire
    }

    fn has_value(&self) -> bool {
        !self.void
    }

    fn parser(&self) -> &Identifier {
        self.parser.as_ref().expect("return parser")
    }

    fn value(&self) -> &Identifier {
        self.value.as_ref().expect("return value")
    }

    fn encoded(c_type: &c::Type, codec: &WritePlan) -> Result<Self> {
        let encoded = OwnedPayload::write(codec)?;
        Self::wire(
            c_type,
            encoded.parser().clone(),
            encoded.direct_vector(),
            encoded.primitive(),
            encoded.has_string(),
            encoded.has_bytes(),
            encoded.has_raw_wire(),
        )
    }

    fn wire(
        c_type: &c::Type,
        parser: Identifier,
        direct_vector: Option<direct_vector::Element>,
        wire_primitive: Option<primitive::Runtime>,
        string: bool,
        bytes: bool,
        raw_wire: bool,
    ) -> Result<Self> {
        Ok(Self {
            c_type: TypeSyntax::new(c_type).anonymous()?,
            parser: Some(parser),
            default_value: Expression::literal(Literal::compound_zero()),
            value: Some(Identifier::parse("return_value")?),
            primitive: None,
            wire_primitive,
            direct_vector,
            wire: true,
            string,
            bytes,
            raw_wire,
            void: false,
        })
    }
}

struct ClosureReturnValue<'ctype, 'bridge, 'context, 'bindings> {
    c_type: &'ctype c::Type,
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for ClosureReturnValue<'_, '_, '_, '_> {
    type Output = Result<ReturnValue>;

    fn void(&mut self) -> Self::Output {
        Ok(ReturnValue {
            c_type: TypeSyntax::new(self.c_type).anonymous()?,
            parser: None,
            default_value: Expression::literal(Literal::compound_zero()),
            value: None,
            primitive: None,
            wire_primitive: None,
            direct_vector: None,
            wire: false,
            string: false,
            bytes: false,
            raw_wire: false,
            void: true,
        })
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan DirectValueType) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Self::unsupported();
        }
        let direct = direct::NativeSlot::from_direct_value(ty, self.bridge, self.context)?;
        Ok(ReturnValue {
            c_type: TypeSyntax::new(self.c_type).anonymous()?,
            parser: Some(direct.parser().clone()),
            default_value: direct.default_value().clone(),
            value: Some(Identifier::parse("return_value")?),
            primitive: direct.primitive(),
            wire_primitive: None,
            direct_vector: None,
            wire: false,
            string: false,
            bytes: false,
            raw_wire: false,
            void: false,
        })
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        codec: &'plan WritePlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::ReturnSlot, native::BufferShape::Buffer) => {
                ReturnValue::encoded(self.c_type, codec)
            }
            (ReturnValueSlot::ReturnSlot, _) | (ReturnValueSlot::OutPointer, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown closure return",
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

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        let primitive = primitive::Runtime::new(primitive);
        ReturnValue::wire(
            self.c_type,
            primitive.optional_wire_encoder()?,
            None,
            Some(primitive),
            false,
            false,
            false,
        )
    }

    fn direct_vector(&mut self, element: &'plan DirectVectorElementType) -> Self::Output {
        let element = direct_vector::Element::from_element(element, self.bridge, self.context)?;
        ReturnValue::wire(
            self.c_type,
            element.vector_encoder().clone(),
            Some(element),
            None,
            false,
            false,
            false,
        )
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl ClosureReturnValue<'_, '_, '_, '_> {
    fn unsupported() -> Result<ReturnValue> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported closure return",
        })
    }
}

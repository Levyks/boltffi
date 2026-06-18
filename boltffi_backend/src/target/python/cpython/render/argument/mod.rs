use boltffi_binding::{
    CallbackId, EnumId, HandlePresence, HandleTarget, IncomingParam, IntoRust, Native, ParamDecl,
    ParamPlanRender, Primitive, Receive, RecordId, TypeRef, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, Type, identifier::Identifier, syntax::TypeSyntax},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Error, RenderContext, Result},
    target::python::{
        cpython::render::{
            callback, closure, direct, direct_vector, enumeration, primitive, record,
        },
        name_style::Name,
    },
};

mod buffered;

pub use self::buffered::MutationOutput;
use self::buffered::{BufferedArgument, RegisteredObject};

pub struct Conversion {
    index: usize,
    name: String,
    kind: Kind,
    primitive: Option<primitive::Runtime>,
}

impl Conversion {
    pub fn from_parameter(
        owner: &str,
        index: usize,
        parameter: &ParamDecl<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Identifier::escape(Name::new(parameter.name()).function())?.to_string();
        match parameter.payload() {
            IncomingParam::Value(plan) => plan.render_with(&mut ParameterConversion {
                index,
                name,
                bridge,
                context,
            }),
            IncomingParam::Closure(closure) => {
                Self::from_closure(owner, index, name, closure, c_parameters, bridge, context)
            }
        }
    }

    pub fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    pub fn class_receiver(carrier: native::HandleCarrier) -> Result<Self> {
        Self::handle_with_name(0, "receiver", carrier)
    }

    pub fn direct_record_receiver(
        record: RecordId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_record_id(record, bridge, context)?;
        Self::direct_with_name(0, "receiver", direct.c_type().to_owned(), direct.parser())
    }

    pub fn encoded_record_receiver(
        record: RecordId,
        receive: Receive,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = record::Symbols::from_record_id(record, bridge, context)?;
        Self::encoded_with_name(
            0,
            "receiver",
            receive,
            BufferedArgument::RegisteredObject(RegisteredObject::new(
                symbols.parser(),
                symbols.boxer(),
            )),
        )
    }

    pub fn c_style_enum_receiver(
        enumeration: EnumId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_enum_id(enumeration, bridge, context)?;
        Self::direct_with_name(0, "receiver", direct.c_type().to_owned(), direct.parser())
    }

    pub fn data_enum_receiver(
        enumeration: EnumId,
        receive: Receive,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = enumeration::Symbols::from_enum_id(enumeration, bridge, context)?;
        Self::encoded_with_name(
            0,
            "receiver",
            receive,
            BufferedArgument::RegisteredObject(RegisteredObject::new(
                symbols.parser(),
                symbols.owned_decoder(),
            )),
        )
    }

    pub fn call_args(&self) -> Vec<String> {
        match &self.kind {
            Kind::Direct(_) => vec![self.name.clone()],
            Kind::Buffered(buffered) => buffered.call_args(),
            Kind::Closure(closure) => closure.call_args().into_iter().collect(),
        }
    }

    pub fn c_arity(&self) -> usize {
        match &self.kind {
            Kind::Direct(_) => 1,
            Kind::Buffered(buffered) => buffered.c_arity(),
            Kind::Closure(_) => closure::Parameter::c_arity(),
        }
    }

    pub const fn index(&self) -> usize {
        self.index
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn is_direct(&self) -> bool {
        matches!(self.kind, Kind::Direct(_))
    }

    pub fn is_encoded(&self) -> bool {
        matches!(self.kind, Kind::Buffered(_))
    }

    pub fn is_closure(&self) -> bool {
        matches!(self.kind, Kind::Closure(_))
    }

    pub fn is_string(&self) -> bool {
        false
    }

    pub fn is_bytes(&self) -> bool {
        false
    }

    pub fn is_raw_wire(&self) -> bool {
        matches!(&self.kind, Kind::Buffered(buffered) if buffered.is_raw_wire())
    }

    pub fn has_closure_string_argument(&self) -> bool {
        matches!(&self.kind, Kind::Closure(closure) if closure.has_string_argument())
    }

    pub fn has_closure_bytes_argument(&self) -> bool {
        matches!(&self.kind, Kind::Closure(closure) if closure.has_bytes_argument())
    }

    pub fn has_closure_raw_wire_argument(&self) -> bool {
        matches!(&self.kind, Kind::Closure(closure) if closure.has_raw_wire_argument())
    }

    pub fn wire_primitive(&self) -> Option<primitive::Runtime> {
        match &self.kind {
            Kind::Buffered(buffered) => buffered.primitive(),
            Kind::Closure(_) | Kind::Direct(_) => None,
        }
    }

    pub fn closure_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        match &self.kind {
            Kind::Closure(closure) => EitherIter::left(closure.primitives()),
            Kind::Direct(_) | Kind::Buffered(_) => EitherIter::right(std::iter::empty()),
        }
    }

    pub fn closure_wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        match &self.kind {
            Kind::Closure(closure) => EitherIter::left(closure.wire_primitives()),
            Kind::Direct(_) | Kind::Buffered(_) => EitherIter::right(std::iter::empty()),
        }
    }

    pub fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        match &self.kind {
            Kind::Buffered(buffered) => buffered.direct_vector_element(),
            Kind::Closure(_) | Kind::Direct(_) => None,
        }
    }

    pub fn closure_direct_vector_elements(
        &self,
    ) -> impl Iterator<Item = direct_vector::Element> + '_ {
        match &self.kind {
            Kind::Closure(closure) => EitherIter::left(closure.direct_vector_elements()),
            Kind::Direct(_) | Kind::Buffered(_) => EitherIter::right(std::iter::empty()),
        }
    }

    pub fn c_type(&self) -> &str {
        match &self.kind {
            Kind::Direct(direct) => direct.c_type.as_str(),
            Kind::Buffered(_) | Kind::Closure(_) => "",
        }
    }

    pub fn parser(&self) -> &str {
        match &self.kind {
            Kind::Direct(direct) => direct.parser.as_str(),
            Kind::Buffered(buffered) => buffered.parser.as_str(),
            Kind::Closure(closure) => closure.parser(),
        }
    }

    pub fn wire(&self) -> &str {
        match &self.kind {
            Kind::Direct(_) => "",
            Kind::Buffered(buffered) => buffered.wire.as_str(),
            Kind::Closure(_) => "",
        }
    }

    pub fn pointer(&self) -> &str {
        match &self.kind {
            Kind::Direct(_) => "",
            Kind::Buffered(buffered) => buffered.pointer.as_str(),
            Kind::Closure(_) => "",
        }
    }

    pub fn length(&self) -> &str {
        match &self.kind {
            Kind::Direct(_) => "",
            Kind::Buffered(buffered) => buffered.length.as_str(),
            Kind::Closure(_) => "",
        }
    }

    pub fn has_mutation(&self) -> bool {
        matches!(&self.kind, Kind::Buffered(buffered) if buffered.mutation.is_some())
    }

    pub fn mutation_buffer(&self) -> &str {
        match &self.kind {
            Kind::Buffered(buffered) => buffered
                .mutation
                .as_ref()
                .map(MutationOutput::buffer)
                .unwrap_or(""),
            Kind::Direct(_) | Kind::Closure(_) => "",
        }
    }

    pub fn mutation(&self) -> Option<MutationOutput> {
        match &self.kind {
            Kind::Buffered(buffered) => buffered.mutation.clone(),
            Kind::Direct(_) | Kind::Closure(_) => None,
        }
    }

    pub fn closure_declaration(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_call_declaration(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.call_declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_call(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.call(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_context_declaration(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.context_declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_context(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.context(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_release_declaration(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.release_declaration(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_release(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.release(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    pub fn closure_release_needed(&self) -> &str {
        match &self.kind {
            Kind::Closure(closure) => closure.release_needed(),
            Kind::Direct(_) | Kind::Buffered(_) => "",
        }
    }

    fn from_direct_slot(index: usize, name: String, direct: direct::NativeSlot) -> Result<Self> {
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type: direct.c_type().to_owned(),
                parser: direct.parser().to_owned(),
            }),
            primitive: direct.primitive(),
        })
    }

    fn from_handle(index: usize, name: String, carrier: native::HandleCarrier) -> Result<Self> {
        Self::handle_with_name(index, name, carrier)
    }

    fn handle_with_name(
        index: usize,
        name: impl Into<String>,
        carrier: native::HandleCarrier,
    ) -> Result<Self> {
        let name = name.into();
        let carrier = primitive::Runtime::native_handle(carrier)?;
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type: carrier.c_type()?.to_owned(),
                parser: carrier.parser()?.to_owned(),
            }),
            primitive: Some(carrier),
        })
    }

    fn from_callback(
        index: usize,
        name: String,
        callback: CallbackId,
        presence: HandlePresence,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = callback::Symbols::from_callback_id(callback, bridge, context)?;
        Ok(Self {
            index,
            name,
            kind: Kind::Direct(Direct {
                c_type: TypeSyntax::new(&Type::CallbackHandle).anonymous()?,
                parser: symbols.parser(presence).to_owned(),
            }),
            primitive: None,
        })
    }

    fn from_closure(
        owner: &str,
        index: usize,
        name: String,
        closure: &boltffi_binding::ClosureParameter<Native, IntoRust>,
        c_parameters: &[c::Parameter],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let closure = closure::Parameter::new(
            owner,
            index,
            name.clone(),
            closure,
            c_parameters,
            bridge,
            context,
        )?;
        Ok(Self {
            index,
            name,
            kind: Kind::Closure(Box::new(closure)),
            primitive: None,
        })
    }

    fn encoded(
        index: usize,
        name: String,
        receive: Receive,
        encoded: BufferedArgument,
    ) -> Result<Self> {
        Self::encoded_with_name(index, name, receive, encoded)
    }

    fn direct_with_name(
        index: usize,
        name: impl Into<String>,
        c_type: String,
        parser: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            index,
            name: name.into(),
            kind: Kind::Direct(Direct {
                c_type,
                parser: parser.into(),
            }),
            primitive: None,
        })
    }

    fn encoded_with_name(
        index: usize,
        name: impl Into<String>,
        receive: Receive,
        encoded: BufferedArgument,
    ) -> Result<Self> {
        let name = name.into();
        let wire = format!("{name}_wire");
        let pointer = format!("{name}_ptr");
        let length = format!("{name}_len");
        let mutation = match receive {
            Receive::ByMutRef => encoded.mutation_output(&name)?,
            Receive::ByValue | Receive::ByRef => None,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown encoded parameter receive mode",
                });
            }
        };
        let parser = encoded.parser()?;
        let primitive = encoded.primitive();
        Ok(Self {
            index,
            name,
            kind: Kind::Buffered(Box::new(BufferedParam {
                argument: encoded,
                parser,
                wire,
                pointer,
                length,
                mutation,
            })),
            primitive,
        })
    }
}

struct ParameterConversion<'bridge, 'context, 'bindings> {
    index: usize,
    name: String,
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl ParameterConversion<'_, '_, '_> {
    fn direct_type(&self, ty: &TypeRef, receive: Receive) -> Result<Conversion> {
        match receive {
            Receive::ByValue | Receive::ByRef => Conversion::from_direct_slot(
                self.index,
                self.name.clone(),
                direct::NativeSlot::from_type_ref(
                    ty,
                    self.bridge,
                    self.context,
                    "unsupported direct parameter",
                )?,
            ),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "borrowed direct parameter",
            }),
        }
    }

    fn handle_type(
        &self,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        receive: Receive,
    ) -> Result<Conversion> {
        match (target, carrier, receive) {
            (HandleTarget::Class(_), carrier, _) => {
                Conversion::from_handle(self.index, self.name.clone(), carrier)
            }
            (
                HandleTarget::Callback(callback),
                native::HandleCarrier::CallbackHandle,
                Receive::ByValue,
            ) => Conversion::from_callback(
                self.index,
                self.name.clone(),
                *callback,
                presence,
                self.bridge,
                self.context,
            ),
            (HandleTarget::Callback(_), _, _) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported callback handle parameter",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown handle parameter",
            }),
        }
    }
}

impl<'plan> ParamPlanRender<'plan, Native, IntoRust> for ParameterConversion<'_, '_, '_> {
    type Output = Result<Conversion>;

    fn direct(&mut self, ty: &TypeRef, receive: Receive) -> Self::Output {
        self.direct_type(ty, receive)
    }

    fn encoded(
        &mut self,
        _: &TypeRef,
        _: &WritePlan,
        shape: native::BufferShape,
        receive: Receive,
    ) -> Self::Output {
        match shape {
            native::BufferShape::Slice => Conversion::encoded(
                self.index,
                self.name.clone(),
                receive,
                BufferedArgument::RawWire,
            ),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported encoded parameter",
            }),
        }
    }

    fn handle(
        &mut self,
        target: &HandleTarget,
        carrier: native::HandleCarrier,
        presence: HandlePresence,
        receive: Receive,
    ) -> Self::Output {
        self.handle_type(target, carrier, presence, receive)
    }

    fn scalar_option(&mut self, primitive: Primitive) -> Self::Output {
        Conversion::encoded(
            self.index,
            self.name.clone(),
            Receive::ByValue,
            BufferedArgument::OptionalPrimitive(primitive::Runtime::new(primitive)),
        )
    }

    fn direct_vector(&mut self, element: &TypeRef) -> Self::Output {
        Conversion::encoded(
            self.index,
            self.name.clone(),
            Receive::ByValue,
            BufferedArgument::DirectVector(direct_vector::Element::from_type_ref(
                element,
                self.bridge,
                self.context,
            )?),
        )
    }
}

enum Kind {
    Direct(Direct),
    Buffered(Box<BufferedParam>),
    Closure(Box<closure::Parameter>),
}

struct Direct {
    c_type: String,
    parser: String,
}

enum EitherIter<Left, Right> {
    Left(Left),
    Right(Right),
}

impl<Left, Right> EitherIter<Left, Right> {
    fn left(left: Left) -> Self {
        Self::Left(left)
    }

    fn right(right: Right) -> Self {
        Self::Right(right)
    }
}

impl<Item, Left, Right> Iterator for EitherIter<Left, Right>
where
    Left: Iterator<Item = Item>,
    Right: Iterator<Item = Item>,
{
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Left(left) => left.next(),
            Self::Right(right) => right.next(),
        }
    }
}

struct BufferedParam {
    argument: BufferedArgument,
    parser: String,
    wire: String,
    pointer: String,
    length: String,
    mutation: Option<MutationOutput>,
}

impl BufferedParam {
    fn call_args(&self) -> Vec<String> {
        self.argument
            .call_args(&self.pointer, &self.length, self.mutation.as_ref())
    }

    fn c_arity(&self) -> usize {
        2 + usize::from(self.mutation.is_some())
    }

    fn is_raw_wire(&self) -> bool {
        self.argument.is_raw_wire()
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.argument.primitive()
    }

    fn direct_vector_element(&self) -> Option<direct_vector::Element> {
        self.argument.direct_vector_element()
    }
}

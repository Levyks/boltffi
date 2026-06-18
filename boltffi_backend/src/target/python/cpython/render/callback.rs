use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackDecl, CallbackId, ClosureReturn, ErrorDecl, ExecutionDecl, HandlePresence,
    HandleTarget, ImportedMethodDecl, IntoRust, Native, OutOfRust, OutgoingParam, ParamDecl,
    ParamPlanRender, Primitive, ReadPlan, ReturnPlan, ReturnPlanRender, ReturnValueSlot, TypeRef,
    VTableSlot, WritePlan, native,
};

use crate::{
    bridge::{
        c::{self, identifier::Identifier, syntax::TypeSyntax},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        codec::{BorrowedPayload, OwnedPayload},
        cpython::render::{direct, direct_vector, primitive},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/callback.c", escape = "none")]
struct Template {
    vtable_type: String,
    vtable: String,
    register: String,
    register_storage: String,
    create_handle_storage: String,
    copy_buffer_storage: String,
    parser: String,
    optional_parser: String,
    free: String,
    clone: String,
    slots: Vec<Slot>,
    methods: Vec<Method>,
}

pub struct Callback {
    symbols: Symbols,
    vtable_type: String,
    register_storage: String,
    create_handle_storage: String,
    copy_buffer_storage: String,
    slots: Vec<Slot>,
    methods: Vec<Method>,
}

impl Callback {
    pub fn from_declaration(
        declaration: &CallbackDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_callback =
            bridge
                .source_callback(declaration.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "callback without C bridge vtable",
                })?;
        let register = bridge
            .loaded_function(declaration.protocol().register())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback register symbol not loaded",
            })?;
        let create_handle = bridge
            .loaded_function(declaration.protocol().create_handle())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback handle constructor symbol not loaded",
            })?;
        let copy_buffer = bridge.buffer_from_bytes()?;
        let symbols = Symbols::from_declaration(declaration)?;
        let methods = declaration
            .protocol()
            .vtable()
            .methods()
            .iter()
            .map(|method| Method::new(method, c_callback, &symbols, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let slots = std::iter::once(Slot::new(
            declaration.protocol().vtable().free_slot().as_str(),
            symbols.free(),
        ))
        .chain(std::iter::once(Slot::new(
            declaration.protocol().vtable().clone_slot().as_str(),
            symbols.clone(),
        )))
        .chain(
            methods
                .iter()
                .map(|method| Slot::new(method.slot.as_str(), method.function.as_str())),
        )
        .collect();
        Ok(Self {
            symbols,
            vtable_type: TypeSyntax::new(&c::Type::Named(c_callback.vtable().name().to_owned()))
                .anonymous()?,
            register_storage: register.storage_name().to_owned(),
            create_handle_storage: create_handle.storage_name().to_owned(),
            copy_buffer_storage: copy_buffer.storage_name().to_owned(),
            slots,
            methods,
        })
    }

    pub fn render(self) -> Result<Emitted> {
        let source = Template {
            vtable_type: self.vtable_type,
            vtable: self.symbols.vtable,
            register: self.symbols.register,
            register_storage: self.register_storage,
            create_handle_storage: self.create_handle_storage,
            copy_buffer_storage: self.copy_buffer_storage,
            parser: self.symbols.parser,
            optional_parser: self.symbols.optional_parser,
            free: self.symbols.free,
            clone: self.symbols.clone,
            slots: self.slots,
            methods: self.methods,
        }
        .render()?;
        Ok(Emitted::primary(source))
    }

    pub fn binding(&self) -> &str {
        &self.symbols.register
    }

    pub fn parser_declarations(&self) -> Vec<String> {
        self.symbols.parser_declarations().into_iter().collect()
    }

    pub fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.methods.iter().flat_map(Method::primitives)
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.methods.iter().flat_map(Method::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.methods.iter().flat_map(Method::direct_vector_elements)
    }

    pub fn has_string_argument(&self) -> bool {
        self.methods.iter().any(Method::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.methods.iter().any(Method::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.methods.iter().any(Method::has_raw_wire_argument)
    }
}

pub struct Symbols {
    parser: String,
    optional_parser: String,
    vtable: String,
    register: String,
    free: String,
    clone: String,
    method_prefix: String,
}

impl Symbols {
    pub fn from_callback_id(
        callback_id: CallbackId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let callback = context
            .callback(callback_id)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback id without declaration",
            })?;
        bridge
            .source_callback(callback_id)
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback id without C bridge vtable",
            })?;
        Self::from_declaration(callback)
    }

    pub fn parser(&self, presence: HandlePresence) -> &str {
        match presence {
            HandlePresence::Required => &self.parser,
            HandlePresence::Nullable => &self.optional_parser,
            _ => &self.parser,
        }
    }

    fn parser_declarations(&self) -> [String; 2] {
        [
            format!(
                "static int {}(PyObject *value, BoltFFICallbackHandle *out);",
                self.parser
            ),
            format!(
                "static int {}(PyObject *value, BoltFFICallbackHandle *out);",
                self.optional_parser
            ),
        ]
    }

    fn from_declaration(callback: &CallbackDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(callback.name()).function())?.to_string();
        let stem = format!("callback_{stem}");
        Ok(Self {
            parser: format!("boltffi_python_parse_{stem}"),
            optional_parser: format!("boltffi_python_parse_optional_{stem}"),
            vtable: format!("boltffi_python_{stem}_vtable"),
            register: format!("boltffi_python_bind_{stem}"),
            free: format!("boltffi_python_{stem}_free"),
            clone: format!("boltffi_python_{stem}_clone"),
            method_prefix: format!("boltffi_python_{stem}"),
        })
    }

    fn free(&self) -> &str {
        &self.free
    }

    fn clone(&self) -> &str {
        &self.clone
    }

    fn method(&self, name: &boltffi_binding::CanonicalName) -> Result<String> {
        Ok(format!(
            "{}_{}",
            self.method_prefix,
            Identifier::escape(Name::new(name).function())?
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Slot {
    name: String,
    function: String,
}

impl Slot {
    fn new(name: impl Into<String>, function: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            function: function.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Method {
    slot: String,
    function: String,
    python_name: String,
    returns: MethodReturn,
    fallible_return: Option<FallibleReturn>,
    completion: Option<AsyncCompletion>,
    wire_payload: bool,
    params: Vec<MethodParam>,
}

impl Method {
    fn new(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        c_callback: &c::Callback,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_field = c_callback
            .vtable()
            .fields()
            .iter()
            .find(|field| field.name() == method.target().as_str())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "callback method without C vtable slot",
            })?;
        let signature = MethodSignature::from_field(c_field)?;
        let arity = method
            .callable()
            .params()
            .iter()
            .map(MethodParam::arity)
            .collect::<Result<Vec<_>>>()?;
        let (c_params, fallible_return, completion) = match method.callable().execution() {
            ExecutionDecl::Synchronous(_) => match method.callable().error() {
                ErrorDecl::None(_) => (signature.value_params(&arity)?, None, None),
                ErrorDecl::EncodedViaReturnSlot { .. } => {
                    let parts = signature
                        .fallible_value_params(method.callable().returns().plan(), &arity)?;
                    let fallible_return = FallibleReturn::new(
                        method.callable().returns().plan(),
                        method.callable().error(),
                        parts.return_out,
                        bridge,
                        context,
                    )?;
                    (parts.params, Some(fallible_return), None)
                }
                _ => {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "callback method error channel",
                    });
                }
            },
            ExecutionDecl::Asynchronous(_) => {
                let parts = signature.async_value_params(&arity)?;
                let completion = AsyncCompletion::new(
                    method.callable().returns().plan(),
                    method.callable().error(),
                    parts.completion,
                    parts.completion_data,
                    bridge,
                    context,
                )?;
                (parts.params, None, Some(completion))
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown callback method execution",
                });
            }
        };
        let params = method
            .callable()
            .params()
            .iter()
            .zip(c_params)
            .map(|(parameter, c_types)| MethodParam::new(parameter, c_types, bridge, context))
            .collect::<Result<Vec<_>>>()?;
        let returns = match &completion {
            Some(_) => MethodReturn::async_void(signature.returns())?,
            None if fallible_return.is_some() => MethodReturn::fallible_error(signature.returns())?,
            None => MethodReturn::new(
                method.callable().returns().plan(),
                signature.returns(),
                bridge,
                context,
            )?,
        };
        let wire_payload = returns.wire
            || fallible_return
                .as_ref()
                .is_some_and(FallibleReturn::uses_wire_payload)
            || completion
                .as_ref()
                .is_some_and(|completion| completion.payload.wire || completion.payload.error_wire);
        Ok(Self {
            slot: method.target().as_str().to_owned(),
            function: symbols.method(method.name())?,
            python_name: Name::new(method.name()).function(),
            returns,
            fallible_return,
            completion,
            wire_payload,
            params,
        })
    }

    fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(MethodParam::primitive)
            .chain(self.returns.primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::primitives),
            )
            .chain(
                self.completion
                    .iter()
                    .filter_map(|completion| completion.payload.primitive()),
            )
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .filter_map(MethodParam::wire_primitive)
            .chain(self.returns.wire_primitive())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::wire_primitives),
            )
            .chain(
                self.completion
                    .iter()
                    .filter_map(|completion| completion.payload.wire_primitive()),
            )
    }

    fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.params
            .iter()
            .filter_map(MethodParam::direct_vector_element)
            .chain(self.returns.direct_vector())
            .chain(
                self.fallible_return
                    .iter()
                    .flat_map(FallibleReturn::direct_vectors),
            )
            .chain(
                self.completion
                    .iter()
                    .filter_map(|completion| completion.payload.direct_vector()),
            )
    }

    fn has_string_argument(&self) -> bool {
        self.params.iter().any(MethodParam::has_string)
            || self.returns.has_string()
            || self.fallible_return.iter().any(FallibleReturn::has_string)
            || self
                .completion
                .iter()
                .any(|completion| completion.payload.has_string())
    }

    fn has_bytes_argument(&self) -> bool {
        self.params.iter().any(MethodParam::has_bytes)
            || self.returns.has_bytes()
            || self.fallible_return.iter().any(FallibleReturn::has_bytes)
            || self
                .completion
                .iter()
                .any(|completion| completion.payload.has_bytes())
    }

    fn has_raw_wire_argument(&self) -> bool {
        self.params.iter().any(MethodParam::has_raw_wire)
            || self.returns.has_raw_wire()
            || self
                .fallible_return
                .iter()
                .any(FallibleReturn::has_raw_wire)
            || self
                .completion
                .iter()
                .any(|completion| completion.payload.has_raw_wire())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodSignature<'field> {
    returns: &'field c::Type,
    params: &'field [c::Type],
}

impl<'field> MethodSignature<'field> {
    fn from_field(field: &'field c::Field) -> Result<Self> {
        match field.ty() {
            c::Type::FunctionPointer { returns, params } => Ok(Self { returns, params }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback vtable slot is not a function pointer",
            }),
        }
    }

    fn returns(&self) -> &c::Type {
        self.returns
    }

    fn value_params(&self, arity: &[usize]) -> Result<Vec<&'field [c::Type]>> {
        let value_param_count = arity.iter().sum::<usize>();
        let value_start =
            self.params
                .len()
                .checked_sub(value_param_count)
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "callback method parameter ABI mismatch",
                })?;
        if value_start == 0 {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback method handle ABI mismatch",
            });
        }
        Ok(arity
            .iter()
            .scan(value_start, |offset, count| {
                let start = *offset;
                *offset += *count;
                Some(&self.params[start..*offset])
            })
            .collect())
    }

    fn async_value_params(&self, arity: &[usize]) -> Result<AsyncSignature<'field>> {
        let value_param_count = arity.iter().sum::<usize>();
        let value_start = 1;
        let value_end = value_start + value_param_count;
        let completion_index = value_end;
        let completion_data_index = completion_index + 1;
        if self.params.len() != completion_data_index + 1 {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback method parameter ABI mismatch",
            });
        }
        if !matches!(self.returns, c::Type::Void) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback method return ABI mismatch",
            });
        }
        Ok(AsyncSignature {
            params: arity
                .iter()
                .scan(value_start, |offset, count| {
                    let start = *offset;
                    *offset += *count;
                    Some(&self.params[start..*offset])
                })
                .collect(),
            completion: &self.params[completion_index],
            completion_data: &self.params[completion_data_index],
        })
    }

    fn fallible_value_params(
        &self,
        plan: &ReturnPlan<Native, IntoRust>,
        arity: &[usize],
    ) -> Result<FallibleSignature<'field>> {
        let return_param_count = Self::return_param_count(plan)?;
        let value_param_count = arity.iter().sum::<usize>();
        let value_start = 1 + return_param_count;
        if self.params.len() != value_start + value_param_count {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible callback method parameter ABI mismatch",
            });
        }
        Ok(FallibleSignature {
            return_out: (return_param_count == 1).then_some(&self.params[1]),
            params: arity
                .iter()
                .scan(value_start, |offset, count| {
                    let start = *offset;
                    *offset += *count;
                    Some(&self.params[start..*offset])
                })
                .collect(),
        })
    }

    fn return_param_count(plan: &ReturnPlan<Native, IntoRust>) -> Result<usize> {
        plan.render_with(&mut CallbackSuccessOutCount)
    }
}

struct CallbackSuccessOutCount;

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for CallbackSuccessOutCount {
    type Output = Result<usize>;

    fn void(&mut self) -> Self::Output {
        Ok(0)
    }

    fn direct(&mut self, slot: ReturnValueSlot, _: &'plan TypeRef) -> Self::Output {
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
        Self::unsupported()
    }

    fn direct_vector(&mut self, _: &'plan TypeRef) -> Self::Output {
        Self::unsupported()
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl CallbackSuccessOutCount {
    fn slot_count(slot: ReturnValueSlot) -> Result<usize> {
        match slot {
            ReturnValueSlot::OutPointer => Ok(1),
            ReturnValueSlot::ReturnSlot => Self::unsupported(),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown fallible callback success slot",
            }),
        }
    }

    fn unsupported() -> Result<usize> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported fallible callback success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AsyncSignature<'field> {
    params: Vec<&'field [c::Type]>,
    completion: &'field c::Type,
    completion_data: &'field c::Type,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleSignature<'field> {
    params: Vec<&'field [c::Type]>,
    return_out: Option<&'field c::Type>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodParam {
    declarations: Vec<String>,
    name: String,
    object: String,
    expression: String,
    primitive: Option<primitive::Runtime>,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    string: bool,
    bytes: bool,
    raw_wire: bool,
}

impl MethodParam {
    fn arity(parameter: &ParamDecl<Native, OutOfRust>) -> Result<usize> {
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut MethodParamArity),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method parameter",
            }),
        }
    }

    fn new(
        parameter: &ParamDecl<Native, OutOfRust>,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let name = Identifier::escape(Name::new(parameter.name()).function())?.to_string();
        let object = format!("{name}_object");
        match parameter.payload() {
            OutgoingParam::Value(plan) => plan.render_with(&mut MethodParamValue {
                name,
                object,
                c_types,
                bridge,
                context,
            }),
            OutgoingParam::Closure(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method parameter",
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
        name: String,
        object: String,
        ty: &TypeRef,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if c_types.len() != 1 {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback direct parameter ABI mismatch",
            });
        }
        let direct = direct::NativeSlot::from_type_ref(
            ty,
            bridge,
            context,
            "unsupported direct callback parameter",
        )?;
        let expression = direct.box_expression(&name);
        Ok(Self {
            declarations: vec![TypeSyntax::new(&c_types[0]).declaration(&name)?],
            name,
            object,
            expression,
            primitive: direct.primitive(),
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        })
    }

    fn encoded(
        name: String,
        object: String,
        codec: &ReadPlan,
        c_types: &[c::Type],
    ) -> Result<Self> {
        let [pointer, length] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback encoded parameter ABI mismatch",
            });
        };
        let pointer_name = format!("{name}_ptr");
        let length_name = format!("{name}_len");
        let payload = BorrowedPayload::read(codec, &pointer_name, &length_name);
        let wire_primitive = payload.primitive();
        let string = payload.has_string();
        let bytes = payload.has_bytes();
        let raw_wire = payload.has_raw_wire();
        let expression = payload.expression();
        Ok(Self {
            declarations: vec![
                TypeSyntax::new(pointer).declaration(&pointer_name)?,
                TypeSyntax::new(length).declaration(&length_name)?,
            ],
            name,
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

    fn direct_vector_param(
        name: String,
        object: String,
        element: &TypeRef,
        c_types: &[c::Type],
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let [pointer, length] = c_types else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback vector parameter ABI mismatch",
            });
        };
        let pointer_name = format!("{name}_ptr");
        let length_name = format!("{name}_len");
        let element = direct_vector::Element::from_type_ref(element, bridge, context)?;
        Ok(Self {
            declarations: vec![
                TypeSyntax::new(pointer).declaration(&pointer_name)?,
                TypeSyntax::new(length).declaration(&length_name)?,
            ],
            name,
            object,
            expression: format!("{}({pointer_name}, {length_name})", element.vector_boxer()),
            primitive: None,
            wire_primitive: None,
            direct_vector: Some(element),
            string: false,
            bytes: false,
            raw_wire: false,
        })
    }
}

struct MethodParamArity;

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for MethodParamArity {
    type Output = Result<usize>;

    fn direct(&mut self, _: &TypeRef, _: ()) -> Self::Output {
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
                shape: "unsupported callback method parameter",
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
        Ok(1)
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported callback method parameter",
        })
    }

    fn direct_vector(&mut self, _: &TypeRef) -> Self::Output {
        Ok(2)
    }
}

struct MethodParamValue<'ctype, 'bridge, 'context, 'bindings> {
    name: String,
    object: String,
    c_types: &'ctype [c::Type],
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ParamPlanRender<'plan, Native, OutOfRust> for MethodParamValue<'_, '_, '_, '_> {
    type Output = Result<MethodParam>;

    fn direct(&mut self, ty: &TypeRef, _: ()) -> Self::Output {
        MethodParam::direct(
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
                MethodParam::encoded(self.name.clone(), self.object.clone(), codec, self.c_types)
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported callback method encoded parameter",
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
            shape: "callback handle method parameter",
        })
    }

    fn scalar_option(&mut self, _: Primitive) -> Self::Output {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unknown callback method parameter",
        })
    }

    fn direct_vector(&mut self, element: &TypeRef) -> Self::Output {
        MethodParam::direct_vector_param(
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
    declarations: Vec<String>,
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
                shape: "callback method error channel",
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

    fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> {
        self.success.primitive().into_iter()
    }

    fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> {
        self.success
            .wire_primitive()
            .into_iter()
            .chain(self.error.wire_primitive())
    }

    fn direct_vectors(&self) -> impl Iterator<Item = direct_vector::Element> {
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
    out: String,
    value: String,
    c_type: String,
    default_value: String,
    parser: String,
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
            out: String::new(),
            value: String::new(),
            c_type: String::new(),
            default_value: String::new(),
            parser: String::new(),
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
        ty: &TypeRef,
        out_type: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_type_ref(
            ty,
            bridge,
            context,
            "unsupported direct callback success",
        )?;
        Ok(Self {
            out: "return_out".to_owned(),
            value: "return_success".to_owned(),
            c_type: TypeSyntax::new(out_type).anonymous()?,
            default_value: direct.default_value().to_owned(),
            parser: direct.parser().to_owned(),
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
                shape: "fallible callback encoded out-parameter",
            });
        }
        let encoded = OwnedPayload::write(codec);
        Ok(Self {
            out: "return_out".to_owned(),
            value: "return_success".to_owned(),
            c_type: String::new(),
            default_value: String::new(),
            parser: encoded.parser().to_owned(),
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
                shape: "fallible callback success out-parameter",
            }),
        }
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
                shape: "void callback success out-parameter",
            });
        }
        Ok(FallibleSuccess::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan TypeRef) -> Self::Output {
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
                shape: "unknown fallible callback success slot",
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
                shape: "unknown fallible callback success slot",
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

    fn direct_vector(&mut self, _: &'plan TypeRef) -> Self::Output {
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
            shape: "unsupported fallible callback success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FallibleError {
    value: String,
    parser: String,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    string: bool,
    bytes: bool,
    raw_wire: bool,
}

impl FallibleError {
    fn new(codec: &WritePlan) -> Result<Self> {
        let encoded = OwnedPayload::write(codec);
        Ok(Self {
            value: "return_value".to_owned(),
            parser: encoded.parser().to_owned(),
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
struct AsyncCompletion {
    declaration: String,
    data_declaration: String,
    callback: String,
    data: String,
    payload: CompletionPayload,
}

impl AsyncCompletion {
    fn new(
        plan: &ReturnPlan<Native, IntoRust>,
        error: &ErrorDecl<Native, IntoRust>,
        completion: &c::Type,
        completion_data: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let signature = CompletionSignature::new(completion)?;
        let payload = match error {
            ErrorDecl::None(_) => {
                CompletionPayload::infallible(plan, signature.payload(), bridge, context)?
            }
            ErrorDecl::EncodedViaReturnSlot { codec, .. } => {
                CompletionPayload::fallible(plan, codec, signature.payload(), bridge, context)?
            }
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "async callback error channel",
                });
            }
        };
        Ok(Self {
            declaration: TypeSyntax::new(completion).declaration("completion")?,
            data_declaration: TypeSyntax::new(completion_data).declaration("completion_data")?,
            callback: "completion".to_owned(),
            data: "completion_data".to_owned(),
            payload,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionSignature<'signature> {
    payload: Option<&'signature c::Type>,
}

impl<'signature> CompletionSignature<'signature> {
    fn new(completion: &'signature c::Type) -> Result<Self> {
        let c::Type::FunctionPointer { returns, params } = completion else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback completion is not a function pointer",
            });
        };
        if !matches!(returns.as_ref(), c::Type::Void) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback completion return ABI mismatch",
            });
        }
        if !matches!(
            params.as_slice(),
            [c::Type::MutPointer(data), c::Type::Status]
                if matches!(data.as_ref(), c::Type::Void)
        ) && !matches!(
            params.as_slice(),
            [c::Type::MutPointer(data), c::Type::Status, _]
                if matches!(data.as_ref(), c::Type::Void)
        ) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback completion parameter ABI mismatch",
            });
        }
        Ok(Self {
            payload: params.get(2),
        })
    }

    fn payload(&self) -> Option<&'signature c::Type> {
        self.payload
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletionPayload {
    value: String,
    c_type: String,
    default_value: String,
    parser: String,
    error_parser: String,
    direct_value: String,
    direct_type: String,
    error_direct_value: String,
    error_direct_type: String,
    wire: bool,
    direct_bytes: bool,
    error_wire: bool,
    error_direct_bytes: bool,
    fallible: bool,
    void: bool,
    primitive: Option<primitive::Runtime>,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    string: bool,
    bytes: bool,
    raw_wire: bool,
}

impl CompletionPayload {
    fn infallible(
        plan: &ReturnPlan<Native, IntoRust>,
        payload: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut AsyncCompletionPayload {
            payload,
            bridge,
            context,
        })
    }

    fn fallible(
        success: &ReturnPlan<Native, IntoRust>,
        error: &WritePlan,
        payload: Option<&c::Type>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let payload = payload.ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "async fallible callback completion without payload",
        })?;
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async fallible callback payload ABI mismatch",
            });
        }
        let success = Self::fallible_success(success, payload, bridge, context)?;
        let error = Self::wire(error, payload)?;
        Ok(Self {
            error_parser: error.parser,
            error_direct_value: "completion_error_direct_value".to_owned(),
            error_direct_type: error.direct_type,
            error_wire: error.wire,
            error_direct_bytes: error.direct_bytes,
            fallible: true,
            ..success
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

    fn void() -> Self {
        Self {
            value: String::new(),
            c_type: String::new(),
            default_value: String::new(),
            parser: String::new(),
            error_parser: String::new(),
            direct_value: String::new(),
            direct_type: String::new(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: false,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
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
        ty: &TypeRef,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let direct = direct::NativeSlot::from_type_ref(
            ty,
            bridge,
            context,
            "unsupported async direct callback payload",
        )?;
        Self {
            value: "completion_value".to_owned(),
            c_type: String::new(),
            default_value: direct.default_value().to_owned(),
            parser: direct.parser().to_owned(),
            error_parser: String::new(),
            direct_value: String::new(),
            direct_type: String::new(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: false,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            primitive: direct.primitive(),
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        }
        .with_payload_type(payload)
    }

    fn wire(codec: &WritePlan, payload: &c::Type) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async wire callback payload ABI mismatch",
            });
        }
        let encoded = OwnedPayload::write(codec);
        Self {
            value: "completion_value".to_owned(),
            c_type: String::new(),
            default_value: "{0}".to_owned(),
            parser: encoded.parser().to_owned(),
            error_parser: String::new(),
            direct_value: String::new(),
            direct_type: String::new(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: true,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            primitive: None,
            wire_primitive: encoded.primitive(),
            direct_vector: encoded.direct_vector(),
            string: encoded.has_string(),
            bytes: encoded.has_bytes(),
            raw_wire: encoded.has_raw_wire(),
        }
        .with_payload_type(payload)
    }

    fn vector(
        element: &TypeRef,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async vector callback payload ABI mismatch",
            });
        }
        let element = direct_vector::Element::from_type_ref(element, bridge, context)?;
        Self {
            value: "completion_value".to_owned(),
            c_type: String::new(),
            default_value: "{0}".to_owned(),
            parser: element.vector_encoder().to_owned(),
            error_parser: String::new(),
            direct_value: String::new(),
            direct_type: String::new(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: true,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            primitive: None,
            wire_primitive: None,
            direct_vector: Some(element),
            string: false,
            bytes: false,
            raw_wire: false,
        }
        .with_payload_type(payload)
    }

    fn scalar_option(primitive: Primitive, payload: &c::Type) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async optional callback payload ABI mismatch",
            });
        }
        let primitive = primitive::Runtime::new(primitive);
        Self {
            value: "completion_value".to_owned(),
            c_type: String::new(),
            default_value: "{0}".to_owned(),
            parser: primitive.optional_wire_encoder()?,
            error_parser: String::new(),
            direct_value: String::new(),
            direct_type: String::new(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: true,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            primitive: None,
            wire_primitive: Some(primitive),
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        }
        .with_payload_type(payload)
    }

    fn fallible_success(
        plan: &ReturnPlan<Native, IntoRust>,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        plan.render_with(&mut AsyncCompletionSuccess {
            payload,
            bridge,
            context,
        })
    }

    fn direct_bytes(
        ty: &TypeRef,
        payload: &c::Type,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async direct-byte callback payload ABI mismatch",
            });
        }
        let direct = direct::NativeSlot::from_type_ref(
            ty,
            bridge,
            context,
            "unsupported async direct-byte callback payload",
        )?;
        Self {
            value: "completion_value".to_owned(),
            c_type: String::new(),
            default_value: "{0}".to_owned(),
            parser: direct.parser().to_owned(),
            error_parser: String::new(),
            direct_value: "completion_direct_value".to_owned(),
            direct_type: direct.c_type().to_owned(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: false,
            direct_bytes: true,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            primitive: None,
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        }
        .with_payload_type(payload)
    }

    fn wire_empty(payload: &c::Type) -> Result<Self> {
        if !matches!(payload, c::Type::Buffer) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async empty callback payload ABI mismatch",
            });
        }
        Self {
            value: "completion_value".to_owned(),
            c_type: String::new(),
            default_value: "{0}".to_owned(),
            parser: String::new(),
            error_parser: String::new(),
            direct_value: String::new(),
            direct_type: String::new(),
            error_direct_value: String::new(),
            error_direct_type: String::new(),
            wire: false,
            direct_bytes: false,
            error_wire: false,
            error_direct_bytes: false,
            fallible: false,
            void: false,
            primitive: None,
            wire_primitive: None,
            direct_vector: None,
            string: false,
            bytes: false,
            raw_wire: false,
        }
        .with_payload_type(payload)
    }

    fn with_payload_type(mut self, payload: &c::Type) -> Result<Self> {
        let payload_type = TypeSyntax::new(payload).anonymous()?;
        self.c_type = payload_type.clone();
        self.default_value = match self.default_value.as_str() {
            "{0}" => format!("({payload_type}){{0}}"),
            value => value.to_owned(),
        };
        Ok(self)
    }
}

struct AsyncCompletionPayload<'payload, 'bridge, 'context, 'bindings> {
    payload: Option<&'payload c::Type>,
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for AsyncCompletionPayload<'_, '_, '_, '_> {
    type Output = Result<CompletionPayload>;

    fn void(&mut self) -> Self::Output {
        if self.payload.is_some() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async void callback completion payload",
            });
        }
        Ok(CompletionPayload::void())
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan TypeRef) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Self::unsupported();
        }
        CompletionPayload::direct(
            ty,
            self.payload.ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "async direct callback completion without payload",
            })?,
            self.bridge,
            self.context,
        )
    }

    fn encoded(
        &mut self,
        slot: ReturnValueSlot,
        _: &'plan TypeRef,
        codec: &'plan WritePlan,
        shape: native::BufferShape,
    ) -> Self::Output {
        match (slot, shape) {
            (ReturnValueSlot::ReturnSlot, native::BufferShape::Buffer) => CompletionPayload::wire(
                codec,
                self.payload.ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "async encoded callback completion without payload",
                })?,
            ),
            (ReturnValueSlot::ReturnSlot, _) | (ReturnValueSlot::OutPointer, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async callback return slot",
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
        CompletionPayload::scalar_option(
            primitive,
            self.payload.ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "async optional callback completion without payload",
            })?,
        )
    }

    fn direct_vector(&mut self, element: &'plan TypeRef) -> Self::Output {
        CompletionPayload::vector(
            element,
            self.payload.ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "async vector callback completion without payload",
            })?,
            self.bridge,
            self.context,
        )
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl AsyncCompletionPayload<'_, '_, '_, '_> {
    fn unsupported() -> Result<CompletionPayload> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported async callback return",
        })
    }
}

struct AsyncCompletionSuccess<'payload, 'bridge, 'context, 'bindings> {
    payload: &'payload c::Type,
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust> for AsyncCompletionSuccess<'_, '_, '_, '_> {
    type Output = Result<CompletionPayload>;

    fn void(&mut self) -> Self::Output {
        CompletionPayload::wire_empty(self.payload)
    }

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan TypeRef) -> Self::Output {
        match slot {
            ReturnValueSlot::OutPointer => {
                CompletionPayload::direct_bytes(ty, self.payload, self.bridge, self.context)
            }
            ReturnValueSlot::ReturnSlot => Self::unsupported(),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async fallible callback success slot",
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
                CompletionPayload::wire(codec, self.payload)
            }
            (ReturnValueSlot::OutPointer, _) | (ReturnValueSlot::ReturnSlot, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async fallible callback success slot",
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

    fn direct_vector(&mut self, _: &'plan TypeRef) -> Self::Output {
        Self::unsupported()
    }

    fn closure(&mut self, _: &'plan ClosureReturn<Native, IntoRust>) -> Self::Output {
        Self::unsupported()
    }
}

impl AsyncCompletionSuccess<'_, '_, '_, '_> {
    fn unsupported() -> Result<CompletionPayload> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported async fallible callback success",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodReturn {
    c_type: String,
    parser: String,
    default_value: String,
    value: String,
    primitive: Option<primitive::Runtime>,
    wire_primitive: Option<primitive::Runtime>,
    direct_vector: Option<direct_vector::Element>,
    wire: bool,
    string: bool,
    bytes: bool,
    raw_wire: bool,
    void: bool,
}

impl MethodReturn {
    fn async_void(c_type: &c::Type) -> Result<Self> {
        Ok(Self {
            c_type: TypeSyntax::new(c_type).anonymous()?,
            parser: String::new(),
            default_value: String::new(),
            value: String::new(),
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

    fn fallible_error(c_type: &c::Type) -> Result<Self> {
        Ok(Self {
            c_type: TypeSyntax::new(c_type).anonymous()?,
            parser: String::new(),
            default_value: "{0}".to_owned(),
            value: "return_value".to_owned(),
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
        plan.render_with(&mut CallbackMethodReturnValue {
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

    fn encoded(c_type: &c::Type, codec: &WritePlan) -> Result<Self> {
        let encoded = OwnedPayload::write(codec);
        Self::wire(
            c_type,
            encoded.parser().to_owned(),
            encoded.direct_vector(),
            encoded.primitive(),
            encoded.has_string(),
            encoded.has_bytes(),
            encoded.has_raw_wire(),
        )
    }

    fn wire(
        c_type: &c::Type,
        parser: String,
        direct_vector: Option<direct_vector::Element>,
        wire_primitive: Option<primitive::Runtime>,
        string: bool,
        bytes: bool,
        raw_wire: bool,
    ) -> Result<Self> {
        Ok(Self {
            c_type: TypeSyntax::new(c_type).anonymous()?,
            parser,
            default_value: "{0}".to_owned(),
            value: "return_value".to_owned(),
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

struct CallbackMethodReturnValue<'ctype, 'bridge, 'context, 'bindings> {
    c_type: &'ctype c::Type,
    bridge: &'bridge PythonCExtBridgeContract,
    context: &'context RenderContext<'bindings, Native>,
}

impl<'plan> ReturnPlanRender<'plan, Native, IntoRust>
    for CallbackMethodReturnValue<'_, '_, '_, '_>
{
    type Output = Result<MethodReturn>;

    fn void(&mut self) -> Self::Output {
        Ok(MethodReturn {
            c_type: TypeSyntax::new(self.c_type).anonymous()?,
            parser: String::new(),
            default_value: String::new(),
            value: String::new(),
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

    fn direct(&mut self, slot: ReturnValueSlot, ty: &'plan TypeRef) -> Self::Output {
        if slot != ReturnValueSlot::ReturnSlot {
            return Self::unsupported();
        }
        let direct = direct::NativeSlot::from_type_ref(
            ty,
            self.bridge,
            self.context,
            "unsupported callback method return",
        )?;
        Ok(MethodReturn {
            c_type: TypeSyntax::new(self.c_type).anonymous()?,
            parser: direct.parser().to_owned(),
            default_value: direct.default_value().to_owned(),
            value: "return_value".to_owned(),
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
                MethodReturn::encoded(self.c_type, codec)
            }
            (ReturnValueSlot::ReturnSlot, _) | (ReturnValueSlot::OutPointer, _) => {
                Self::unsupported()
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method return",
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
        MethodReturn::wire(
            self.c_type,
            primitive.optional_wire_encoder()?,
            None,
            Some(primitive),
            false,
            false,
            false,
        )
    }

    fn direct_vector(&mut self, element: &'plan TypeRef) -> Self::Output {
        let element = direct_vector::Element::from_type_ref(element, self.bridge, self.context)?;
        MethodReturn::wire(
            self.c_type,
            element.vector_encoder().to_owned(),
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

impl CallbackMethodReturnValue<'_, '_, '_, '_> {
    fn unsupported() -> Result<MethodReturn> {
        Err(Error::UnsupportedTarget {
            target: "python",
            shape: "unsupported callback method return",
        })
    }
}

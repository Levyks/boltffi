use askama::Template as AskamaTemplate;
use boltffi_binding::{
    CallbackDecl, CallbackId, DeclarationRef, ErrorDecl, ExecutionDecl, HandlePresence,
    ImportedMethodDecl, IntoRust, Native, OutOfRust, OutgoingParam, ParamDecl, ParamPlan,
    Primitive, ReturnPlan, TypeRef, VTableSlot,
};

use crate::{
    bridge::{
        c::{self, identifier::Identifier, syntax::TypeSyntax},
        python_cext::PythonCExtBridgeContract,
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{cpython::render::primitive, name_style::Name},
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/callback.c", escape = "none")]
struct Template {
    vtable_type: String,
    vtable: String,
    register: String,
    register_storage: String,
    create_handle_storage: String,
    parser: String,
    optional_parser: String,
    free: String,
    clone: String,
    slots: Vec<Slot>,
    methods: Vec<Method>,
}

pub struct Wrapper {
    symbols: Symbols,
    vtable_type: String,
    register_storage: String,
    create_handle_storage: String,
    slots: Vec<Slot>,
    methods: Vec<Method>,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &CallbackDecl<Native>,
        bridge: &PythonCExtBridgeContract,
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
        let symbols = Symbols::from_declaration(declaration)?;
        let methods = declaration
            .protocol()
            .vtable()
            .methods()
            .iter()
            .map(|method| Method::new(method, c_callback, &symbols))
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

    pub fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.methods.iter().flat_map(Method::primitives)
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
            .bindings()
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Callback(callback) if callback.id() == callback_id => {
                    Some(callback)
                }
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
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
    params: Vec<MethodParam>,
}

impl Method {
    fn new(
        method: &ImportedMethodDecl<Native, VTableSlot>,
        c_callback: &c::Callback,
        symbols: &Symbols,
    ) -> Result<Self> {
        if matches!(
            method.callable().execution(),
            ExecutionDecl::Asynchronous(_)
        ) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async callback method",
            });
        }
        if !matches!(method.callable().error(), ErrorDecl::None(_)) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible callback method",
            });
        }
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
        signature.require_value_params(method.callable().params().len())?;
        let params = method
            .callable()
            .params()
            .iter()
            .zip(signature.value_params())
            .map(|(parameter, c_type)| MethodParam::new(parameter, c_type))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            slot: method.target().as_str().to_owned(),
            function: symbols.method(method.name())?,
            python_name: Name::new(method.name()).function(),
            returns: MethodReturn::new(method.callable().returns().plan(), signature.returns())?,
            params,
        })
    }

    fn primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.params
            .iter()
            .map(MethodParam::primitive)
            .chain(self.returns.primitive())
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

    fn require_value_params(&self, expected: usize) -> Result<()> {
        if self.value_param_count() == expected {
            Ok(())
        } else {
            Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback method parameter ABI mismatch",
            })
        }
    }

    fn value_params(&self) -> impl Iterator<Item = &c::Type> {
        self.params.iter().skip(1)
    }

    fn value_param_count(&self) -> usize {
        self.params.len().saturating_sub(1)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodParam {
    declaration: String,
    name: String,
    object: String,
    boxer: &'static str,
    primitive: primitive::Runtime,
}

impl MethodParam {
    fn new(parameter: &ParamDecl<Native, OutOfRust>, c_type: &c::Type) -> Result<Self> {
        let OutgoingParam::Value(ParamPlan::Direct {
            ty: TypeRef::Primitive(primitive),
            ..
        }) = parameter.payload()
        else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported callback method parameter",
            });
        };
        let primitive = primitive::Runtime::new(*primitive);
        let name = Identifier::escape(Name::new(parameter.name()).function())?.to_string();
        Ok(Self {
            declaration: TypeSyntax::new(c_type).declaration(&name)?,
            object: format!("{name}_object"),
            name,
            boxer: primitive.boxer()?,
            primitive,
        })
    }

    fn primitive(&self) -> primitive::Runtime {
        self.primitive
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MethodReturn {
    c_type: String,
    parser: String,
    default_value: String,
    value: String,
    primitive: Option<primitive::Runtime>,
    void: bool,
}

impl MethodReturn {
    fn new(plan: &ReturnPlan<Native, IntoRust>, c_type: &c::Type) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self {
                c_type: TypeSyntax::new(c_type).anonymous()?,
                parser: String::new(),
                default_value: String::new(),
                value: String::new(),
                primitive: None,
                void: true,
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let source_primitive = *primitive;
                let primitive = primitive::Runtime::new(source_primitive);
                Ok(Self {
                    c_type: TypeSyntax::new(c_type).anonymous()?,
                    parser: primitive.parser()?.to_owned(),
                    default_value: Self::default_value(source_primitive),
                    value: "return_value".to_owned(),
                    primitive: Some(primitive),
                    void: false,
                })
            }
            ReturnPlan::DirectViaReturnSlot { .. }
            | ReturnPlan::EncodedViaReturnSlot { .. }
            | ReturnPlan::HandleViaReturnSlot { .. }
            | ReturnPlan::ScalarOptionViaReturnSlot { .. }
            | ReturnPlan::DirectVecViaReturnSlot { .. }
            | ReturnPlan::DirectViaOutPointer { .. }
            | ReturnPlan::EncodedViaOutPointer { .. }
            | ReturnPlan::HandleViaOutPointer { .. }
            | ReturnPlan::ClosureViaOutPointer(_) => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported callback method return",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown callback method return",
            }),
        }
    }

    fn primitive(&self) -> Option<primitive::Runtime> {
        self.primitive
    }

    fn default_value(primitive: Primitive) -> String {
        match primitive {
            Primitive::Bool => "false".to_owned(),
            Primitive::F32 => "0.0f".to_owned(),
            Primitive::F64 => "0.0".to_owned(),
            _ => "0".to_owned(),
        }
    }

    fn has_value(&self) -> bool {
        !self.void
    }
}

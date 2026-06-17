use askama::Template as AskamaTemplate;
use boltffi_binding::{
    DeclarationRef, DirectFieldDecl, DirectRecordDecl, EncodedRecordDecl, ExportedMethodDecl,
    FieldKey, InitializerDecl, Native, NativeSymbol, Receive, RecordDecl, RecordId, TypeRef,
};

use crate::{
    bridge::{
        c::{self, identifier::Identifier, syntax::TypeSyntax},
        python_cext::{ExtensionMethod, MethodFlags, PythonCExtBridgeContract},
    },
    core::{Emitted, Error, RenderContext, Result},
    target::python::{
        cpython::render::{argument, direct_vector, function, primitive, result},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/record.c", escape = "none")]
struct DirectTemplate {
    class_name: String,
    c_type: String,
    type_object: String,
    register_method: String,
    register_wrapper: String,
    parser: String,
    boxer: String,
    fields: Vec<Field>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/encoded_record.c", escape = "none")]
struct EncodedTemplate {
    class_name: String,
    type_object: String,
    register_method: String,
    register_wrapper: String,
    wire_encoder: String,
    owned_decoder: String,
}

pub struct Wrapper {
    symbols: Symbols,
    shape: Shape,
    method: ExtensionMethod,
    callables: Vec<function::Wrapper>,
}

impl Wrapper {
    pub fn from_declaration(
        declaration: &RecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        match declaration {
            RecordDecl::Direct(record) => Self::from_direct(record, bridge, context),
            RecordDecl::Encoded(record) => Self::from_encoded(record, bridge, context),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record",
            }),
        }
    }

    pub fn render(self) -> Result<Emitted> {
        let symbols = self.symbols;
        let source = match self.shape {
            Shape::Direct { fields, .. } => {
                let c_type = symbols.c_type()?.to_owned();
                DirectTemplate {
                    class_name: symbols.class_name,
                    c_type,
                    type_object: symbols.type_object,
                    register_method: symbols.register_method,
                    register_wrapper: symbols.register_wrapper,
                    parser: symbols.parser,
                    boxer: symbols.boxer,
                    fields,
                }
                .render()?
            }
            Shape::Encoded => EncodedTemplate {
                class_name: symbols.class_name,
                type_object: symbols.type_object,
                register_method: symbols.register_method,
                register_wrapper: symbols.register_wrapper,
                wire_encoder: symbols.parser,
                owned_decoder: symbols.boxer,
            }
            .render()?,
        };
        let callables = self
            .callables
            .into_iter()
            .map(function::Wrapper::render)
            .collect::<Result<Vec<_>>>()?;
        Ok(Emitted::primary(
            std::iter::once(source)
                .chain(
                    callables
                        .into_iter()
                        .map(|emitted| emitted.primary_chunk().as_str().to_owned()),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }

    pub fn methods(&self) -> impl Iterator<Item = &ExtensionMethod> {
        std::iter::once(&self.method).chain(self.callables.iter().map(function::Wrapper::method))
    }

    pub fn primitives(&self) -> Vec<primitive::Runtime> {
        let own = match &self.shape {
            Shape::Direct { primitives, .. } => primitives.clone(),
            Shape::Encoded => Vec::new(),
        };
        own.into_iter()
            .chain(
                self.callables
                    .iter()
                    .flat_map(function::Wrapper::primitives),
            )
            .collect()
    }

    pub fn cleanup(&self) -> String {
        format!("Py_CLEAR({})", self.symbols.type_object)
    }

    pub fn needs_owned_buffer(&self) -> bool {
        matches!(self.shape, Shape::Encoded)
    }

    pub fn owned_buffers(&self) -> impl Iterator<Item = result::OwnedBuffer> + '_ {
        self.callables
            .iter()
            .filter_map(function::Wrapper::owned_buffer)
    }

    pub fn wire_primitives(&self) -> impl Iterator<Item = primitive::Runtime> + '_ {
        self.callables
            .iter()
            .flat_map(function::Wrapper::wire_primitives)
    }

    pub fn direct_vector_elements(&self) -> impl Iterator<Item = direct_vector::Element> + '_ {
        self.callables
            .iter()
            .flat_map(function::Wrapper::direct_vector_elements)
    }

    pub fn has_string_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_string_argument)
    }

    pub fn has_bytes_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_bytes_argument)
    }

    pub fn has_raw_wire_argument(&self) -> bool {
        self.callables
            .iter()
            .any(function::Wrapper::has_raw_wire_argument)
    }

    fn from_direct(
        record: &DirectRecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(record.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct record without C typedef",
                })?;
        if record.fields().len() != c_record.fields().len() {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "record field mismatch",
            });
        }
        let symbols = Symbols::from_direct(record, c_record)?;
        let fields = record
            .fields()
            .iter()
            .zip(c_record.fields())
            .map(|(source, c_field)| Field::new(source, c_field))
            .collect::<Result<Vec<_>>>()?;
        let primitives = fields.iter().map(Field::primitive).collect();
        let method = ExtensionMethod::new(
            symbols.register_method.clone(),
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::direct_callables(record, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::Direct { primitives, fields },
            method,
            callables,
        })
    }

    fn from_encoded(
        record: &EncodedRecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let symbols = Symbols::from_encoded(record)?;
        let method = ExtensionMethod::new(
            symbols.register_method.clone(),
            symbols.register_wrapper.clone(),
            MethodFlags::FastCall,
        )?;
        let callables = Self::encoded_callables(record, &symbols, bridge, context)?;
        Ok(Self {
            symbols,
            shape: Shape::Encoded,
            method,
            callables,
        })
    }

    fn direct_callables(
        record: &DirectRecordDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Wrapper>> {
        let initializers = record
            .initializers()
            .iter()
            .filter(|initializer| function::Wrapper::supports(initializer.callable()))
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = record
            .methods()
            .iter()
            .filter(|method| {
                function::Wrapper::supports(method.callable())
                    && !matches!(method.callable().receiver(), Some(Receive::ByMutRef))
            })
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|_| {
                        argument::Conversion::direct_record_receiver(record.id(), bridge, context)
                    })
                    .transpose()?
                    .into_iter()
                    .collect();
                Self::method(method, symbols, receiver, bridge, context)
            });
        initializers.chain(methods).collect()
    }

    fn encoded_callables(
        record: &EncodedRecordDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Vec<function::Wrapper>> {
        let initializers = record
            .initializers()
            .iter()
            .filter(|initializer| function::Wrapper::supports(initializer.callable()))
            .map(|initializer| Self::initializer(initializer, symbols, bridge, context));
        let methods = record
            .methods()
            .iter()
            .filter(|method| {
                function::Wrapper::supports(method.callable())
                    && !matches!(method.callable().receiver(), Some(Receive::ByMutRef))
            })
            .map(|method| {
                let receiver = method
                    .callable()
                    .receiver()
                    .map(|receive| {
                        argument::Conversion::encoded_record_receiver(
                            record.id(),
                            receive,
                            bridge,
                            context,
                        )
                    })
                    .transpose()?
                    .into_iter()
                    .collect();
                Self::method(method, symbols, receiver, bridge, context)
            });
        initializers.chain(methods).collect()
    }

    fn initializer(
        initializer: &InitializerDecl<Native>,
        symbols: &Symbols,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<function::Wrapper> {
        function::Wrapper::from_export(
            symbols.initializer(initializer.name()),
            initializer.symbol(),
            initializer.callable(),
            Vec::new(),
            bridge,
            context,
        )
    }

    fn method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        symbols: &Symbols,
        receiver: Vec<argument::Conversion>,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<function::Wrapper> {
        function::Wrapper::from_export(
            symbols.method(method.name()),
            method.target(),
            method.callable(),
            receiver,
            bridge,
            context,
        )
    }
}

pub struct Symbols {
    class_name: String,
    stem: String,
    c_type: Option<String>,
    type_object: String,
    register_method: String,
    register_wrapper: String,
    parser: String,
    boxer: String,
    direct_vec_parser: Option<String>,
    direct_vec_decoder: Option<String>,
}

impl Symbols {
    pub fn from_record_id(
        record_id: RecordId,
        bridge: &PythonCExtBridgeContract,
        context: &RenderContext<Native>,
    ) -> Result<Self> {
        let record = context
            .bindings()
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Record(RecordDecl::Direct(record)) if record.id() == record_id => {
                    Some(RecordDecl::Direct(record.clone()))
                }
                DeclarationRef::Record(RecordDecl::Encoded(record)) if record.id() == record_id => {
                    Some(RecordDecl::Encoded(record.clone()))
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
                shape: "record id without declaration",
            })?;
        match record {
            RecordDecl::Direct(record) => {
                let c_record =
                    bridge
                        .source_direct_record(record_id)
                        .ok_or(Error::UnsupportedTarget {
                            target: "python",
                            shape: "direct record without C typedef",
                        })?;
                Self::from_direct(&record, c_record)
            }
            RecordDecl::Encoded(record) => Self::from_encoded(&record),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown record declaration",
            }),
        }
    }

    pub fn c_type(&self) -> Result<&str> {
        self.c_type.as_deref().ok_or(Error::UnsupportedTarget {
            target: "python",
            shape: "encoded record has no C type",
        })
    }

    pub fn parser(&self) -> &str {
        &self.parser
    }

    pub fn boxer(&self) -> &str {
        &self.boxer
    }

    pub fn direct_vec_parser(&self) -> Result<&str> {
        self.direct_vec_parser
            .as_deref()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded record has no direct vector parser",
            })
    }

    pub fn direct_vec_decoder(&self) -> Result<&str> {
        self.direct_vec_decoder
            .as_deref()
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded record has no direct vector decoder",
            })
    }

    pub fn class_name(&self) -> &str {
        &self.class_name
    }

    pub fn register_method(&self) -> &str {
        &self.register_method
    }

    pub fn initializer(&self, name: &boltffi_binding::CanonicalName) -> String {
        self.callable(name)
    }

    pub fn method(&self, name: &boltffi_binding::CanonicalName) -> String {
        self.callable(name)
    }

    pub fn from_direct(record: &DirectRecordDecl<Native>, c_record: &c::Record) -> Result<Self> {
        let stem = Identifier::escape(Name::new(record.name()).function())?.to_string();
        Ok(Self {
            class_name: Name::new(record.name()).class(),
            stem: stem.clone(),
            c_type: Some(TypeSyntax::new(&c::Type::Named(c_record.name().to_owned())).anonymous()?),
            type_object: format!("boltffi_python_{stem}_type"),
            register_method: format!("_register_{stem}"),
            register_wrapper: format!("boltffi_python_wrapper_register_{stem}"),
            parser: format!("boltffi_python_parse_{stem}"),
            boxer: format!("boltffi_python_box_{stem}"),
            direct_vec_parser: Some(format!("boltffi_python_parse_vec_{stem}")),
            direct_vec_decoder: Some(format!("boltffi_python_decode_owned_vec_{stem}")),
        })
    }

    pub fn from_encoded(record: &EncodedRecordDecl<Native>) -> Result<Self> {
        let stem = Identifier::escape(Name::new(record.name()).function())?.to_string();
        Ok(Self {
            class_name: Name::new(record.name()).class(),
            stem: stem.clone(),
            c_type: None,
            type_object: format!("boltffi_python_{stem}_type"),
            register_method: format!("_register_{stem}"),
            register_wrapper: format!("boltffi_python_wrapper_register_{stem}"),
            parser: format!("boltffi_python_wire_{stem}"),
            boxer: format!("boltffi_python_decode_owned_{stem}"),
            direct_vec_parser: None,
            direct_vec_decoder: None,
        })
    }

    fn callable(&self, name: &boltffi_binding::CanonicalName) -> String {
        format!("_boltffi_{}_{}", self.stem, Name::new(name).function())
    }
}

enum Shape {
    Direct {
        fields: Vec<Field>,
        primitives: Vec<primitive::Runtime>,
    },
    Encoded,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Field {
    python_name: String,
    c_name: String,
    value_name: String,
    parser: &'static str,
    boxer: &'static str,
    primitive: primitive::Runtime,
}

impl Field {
    fn new(source: &DirectFieldDecl, c_field: &c::Field) -> Result<Self> {
        let TypeRef::Primitive(primitive) = source.ty() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "non-primitive direct record field",
            });
        };
        let primitive = primitive::Runtime::new(*primitive);
        let python_name = Self::python_name(source.key())?;
        Ok(Self {
            value_name: Identifier::escape(format!("{python_name}_value"))?.to_string(),
            python_name,
            c_name: c_field.name().to_owned(),
            parser: primitive.parser()?,
            boxer: primitive.boxer()?,
            primitive,
        })
    }

    fn primitive(&self) -> primitive::Runtime {
        self.primitive
    }

    fn python_name(key: &FieldKey) -> Result<String> {
        Ok(match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => format!("field_{position}"),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown record field key",
                });
            }
        })
    }
}

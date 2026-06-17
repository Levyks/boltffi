use std::path::PathBuf;

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    Bindings, CStyleEnumDecl, ClassDecl, ClassId, DeclarationRef, DirectFieldDecl,
    DirectRecordDecl, EnumDecl, EnumId, ErrorDecl, ExportedMethodDecl, FieldKey, FunctionDecl,
    HandlePresence, HandleTarget, IncomingParam, InitializerDecl, IntoRust, Native, NativeSymbol,
    OutOfRust, ParamDecl, ParamPlan, Primitive, RecordDecl, RecordId, ReturnPlan, StreamDecl,
    StreamItemPlan, StreamMode, TypeRef, native,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, FilePath, GeneratedFile, GeneratedOutput, Result},
    target::python::{
        cpython::render::{class, enumeration, record, stream},
        name_style::Name,
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.py", escape = "none")]
struct InitTemplate {
    module_name_literal: String,
    package_name_literal: String,
    package_version_literal: String,
    library_name: String,
    direct_records: Vec<DirectRecordClass>,
    enums: Vec<EnumClass>,
    classes: Vec<Class>,
    functions: Vec<String>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.pyi", escape = "none")]
struct StubTemplate {
    direct_records: Vec<DirectRecordClass>,
    enums: Vec<EnumClass>,
    classes: Vec<Class>,
    functions: Vec<FunctionStub>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/pyproject.toml", escape = "none")]
struct PyprojectTemplate;

#[derive(AskamaTemplate)]
#[template(path = "target/python/setup.py", escape = "none")]
struct SetupTemplate {
    module_name_literal: String,
    package_name_literal: String,
    package_version_literal: String,
    extension_name_literal: String,
    extension_source_literal: String,
}

pub struct Package<'binding, 'bridge> {
    bindings: &'binding Bindings<Native>,
    bridge: &'bridge PythonCExtBridgeContract,
}

impl<'binding, 'bridge> Package<'binding, 'bridge> {
    pub fn new(
        bindings: &'binding Bindings<Native>,
        bridge: &'bridge PythonCExtBridgeContract,
    ) -> Self {
        Self { bindings, bridge }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let module = self.module_name();
        let package = self.package_name();
        let version = self.package_version();
        let direct_records = self.direct_records()?;
        let enums = self.enums()?;
        let classes = self.classes()?;
        let functions = self.functions();
        let stubs = functions
            .iter()
            .map(|function| FunctionStub::from_declaration(function, &self))
            .collect::<Result<Vec<_>>>()?;
        let names = stubs
            .iter()
            .map(|function| function.python_name.clone())
            .collect();
        Ok(GeneratedOutput::new(
            vec![
                self.file("pyproject.toml", PyprojectTemplate.render()?)?,
                self.file(
                    "setup.py",
                    SetupTemplate {
                        module_name_literal: Self::literal(&module),
                        package_name_literal: Self::literal(&package),
                        package_version_literal: Self::literal(
                            version.as_deref().unwrap_or("0.0.0"),
                        ),
                        extension_name_literal: Self::literal(format!(
                            "{}.{}",
                            module,
                            self.bridge.module().as_str()
                        )),
                        extension_source_literal: Self::literal(
                            self.bridge.source_path().as_path().display().to_string(),
                        ),
                    }
                    .render()?,
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.py"),
                    InitTemplate {
                        module_name_literal: Self::literal(&module),
                        package_name_literal: Self::literal(&package),
                        package_version_literal: version
                            .as_deref()
                            .map(Self::literal)
                            .unwrap_or_else(|| "None".to_owned()),
                        library_name: module.clone(),
                        direct_records: direct_records.clone(),
                        enums: enums.clone(),
                        classes: classes.clone(),
                        functions: names,
                    }
                    .render()?,
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.pyi"),
                    StubTemplate {
                        direct_records,
                        enums,
                        classes,
                        functions: stubs,
                    }
                    .render()?,
                )?,
                self.file(PathBuf::from(&module).join("py.typed"), String::new())?,
            ],
            Vec::new(),
        ))
    }

    fn module_name(&self) -> String {
        Name::new(self.bindings.package().name()).function()
    }

    fn package_name(&self) -> String {
        self.module_name()
    }

    fn package_version(&self) -> Option<String> {
        self.bindings.package().version().map(str::to_owned)
    }

    fn functions(&self) -> Vec<&'binding FunctionDecl<Native>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Function(function) => Some(function),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .collect()
    }

    fn direct_records(&self) -> Result<Vec<DirectRecordClass>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Record(record) => Some(record),
                DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|record| match record {
                RecordDecl::Direct(record) => DirectRecordClass::from_direct(record, self.bridge),
                RecordDecl::Encoded(_) => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "encoded record package",
                }),
                _ => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown record package",
                }),
            })
            .collect()
    }

    fn enums(&self) -> Result<Vec<EnumClass>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(enumeration) => Some(enumeration),
                DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|enumeration| match enumeration {
                EnumDecl::CStyle(enumeration) => EnumClass::from_c_style(enumeration, self.bridge),
                EnumDecl::Data(_) => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "data enum package",
                }),
                _ => Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown enum package",
                }),
            })
            .collect()
    }

    fn classes(&self) -> Result<Vec<Class>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Class(class) => Some(class),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|class| Class::from_declaration(class, self))
            .collect()
    }

    fn streams_for_class(&self, class: ClassId) -> Vec<&'binding StreamDecl<Native>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Stream(stream) if stream.owner() == Some(class) => Some(stream),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .collect()
    }

    fn record_name(&self, record_id: RecordId) -> Result<String> {
        self.bindings
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Record(RecordDecl::Direct(record)) if record.id() == record_id => {
                    Some(record)
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
            .map(|record| Name::new(record.name()).class())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "record type hint without direct declaration",
            })
    }

    fn enum_name(&self, enum_id: EnumId) -> Result<String> {
        self.bindings
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration))
                    if enumeration.id() == enum_id =>
                {
                    Some(enumeration)
                }
                DeclarationRef::Enum(_)
                | DeclarationRef::Record(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|enumeration| Name::new(enumeration.name()).class())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum type hint without c-style declaration",
            })
    }

    fn class_name(&self, class_id: &ClassId) -> Result<String> {
        self.bindings
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Class(class) if class.id() == *class_id => Some(class),
                DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Function(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|class| Name::new(class.name()).class())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "class type hint without declaration",
            })
    }

    fn file(&self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Result<GeneratedFile> {
        Ok(GeneratedFile::new(FilePath::new(path.into())?, contents))
    }

    fn literal(value: impl AsRef<str>) -> String {
        format!("{:?}", value.as_ref())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionStub {
    python_name: String,
    parameters: Vec<ParameterStub>,
    return_annotation: String,
}

impl FunctionStub {
    fn from_declaration(
        function: &FunctionDecl<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        if !matches!(function.callable().error(), ErrorDecl::None(_)) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible function stub",
            });
        }
        Ok(Self {
            python_name: Name::new(function.name()).function(),
            parameters: function
                .callable()
                .params()
                .iter()
                .map(|parameter| ParameterStub::from_declaration(parameter, package))
                .collect::<Result<Vec<_>>>()?,
            return_annotation: PythonTypeHint::from_return(
                function.callable().returns().plan(),
                package,
            )?
            .into_string(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DirectRecordClass {
    class_name: String,
    register_method: String,
    fields: Vec<RecordField>,
}

impl DirectRecordClass {
    fn from_direct(
        record: &DirectRecordDecl<Native>,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        let c_record =
            bridge
                .source_direct_record(record.id())
                .ok_or(Error::UnsupportedTarget {
                    target: "python",
                    shape: "direct record package without C typedef",
                })?;
        let symbols = record::Symbols::from_direct(record, c_record)?;
        Ok(Self {
            class_name: symbols.class_name().to_owned(),
            register_method: symbols.register_method().to_owned(),
            fields: record
                .fields()
                .iter()
                .map(RecordField::from_direct)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordField {
    name: String,
    annotation: String,
}

impl RecordField {
    fn from_direct(field: &DirectFieldDecl) -> Result<Self> {
        let TypeRef::Primitive(primitive) = field.ty() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "non-primitive record field annotation",
            });
        };
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: PythonTypeHint::from_primitive(*primitive)?.into_string(),
        })
    }

    fn name(key: &FieldKey) -> Result<String> {
        Ok(match key {
            FieldKey::Named(name) => Name::new(name).function(),
            FieldKey::Position(position) => format!("field_{position}"),
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown record field annotation",
                });
            }
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EnumClass {
    class_name: String,
    register_method: String,
    variants: Vec<EnumVariant>,
}

impl EnumClass {
    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        bridge: &PythonCExtBridgeContract,
    ) -> Result<Self> {
        let class = enumeration::PythonClass::from_c_style(enumeration, bridge)?;
        Ok(Self {
            class_name: class.class_name().to_owned(),
            register_method: class.register_method().to_owned(),
            variants: class
                .variants()
                .iter()
                .map(EnumVariant::from_variant)
                .collect(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EnumVariant {
    name: String,
    value: i128,
}

impl EnumVariant {
    fn from_variant(variant: &enumeration::PythonVariant) -> Self {
        Self {
            name: variant.name().to_owned(),
            value: variant.value(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Class {
    class_name: String,
    release_method: String,
    init: Vec<ClassCallable>,
    constructors: Vec<ClassCallable>,
    static_methods: Vec<ClassCallable>,
    instance_methods: Vec<ClassCallable>,
    streams: Vec<ClassStream>,
}

impl Class {
    fn from_declaration(
        declaration: &ClassDecl<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let symbols = class::Symbols::new(declaration);
        let class_name = symbols.class_name().to_owned();
        let constructors = declaration
            .initializers()
            .iter()
            .map(|initializer| ClassCallable::from_initializer(initializer, &symbols, package))
            .collect::<Result<Vec<_>>>()?;
        let init = constructors
            .iter()
            .filter(|constructor| constructor.python_name == "new")
            .cloned()
            .collect::<Vec<_>>();
        let constructors = constructors
            .into_iter()
            .filter(|constructor| constructor.python_name != "new")
            .collect::<Vec<_>>();
        let methods = declaration
            .methods()
            .iter()
            .map(|method| ClassCallable::from_method(method, &symbols, package))
            .collect::<Result<Vec<_>>>()?;
        let (instance_methods, static_methods): (Vec<_>, Vec<_>) =
            methods.into_iter().partition(|method| method.receiver);
        let streams = package
            .streams_for_class(declaration.id())
            .into_iter()
            .map(|stream| ClassStream::from_declaration(stream, &class_name, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name,
            release_method: symbols.release(),
            init,
            constructors,
            static_methods,
            instance_methods,
            streams,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassStream {
    python_name: String,
    subscribe_method: String,
    subscription_class: String,
    item_annotation: String,
    pop_batch_method: String,
    wait_method: String,
    unsubscribe_method: String,
    free_method: String,
}

impl ClassStream {
    fn from_declaration(
        declaration: &StreamDecl<Native>,
        class_name: &str,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        if matches!(declaration.mode(), StreamMode::Callback) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "callback-mode stream package",
            });
        }
        let StreamItemPlan::Direct { ty, .. } = declaration.item() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "encoded stream package",
            });
        };
        let symbols = stream::Symbols::new(declaration);
        Ok(Self {
            python_name: Name::new(declaration.name()).function(),
            subscribe_method: symbols.subscribe(),
            subscription_class: format!(
                "{}{}Subscription",
                class_name,
                Name::new(declaration.name()).class()
            ),
            item_annotation: PythonTypeHint::from_type_ref(ty, package)?.into_string(),
            pop_batch_method: symbols.pop_batch(),
            wait_method: symbols.wait(),
            unsubscribe_method: symbols.unsubscribe(),
            free_method: symbols.free(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassCallable {
    receiver: bool,
    python_name: String,
    native_name: String,
    parameters: Vec<ParameterStub>,
    arguments: String,
    return_annotation: String,
    returns_value: bool,
    return_class: String,
    wraps_return_handle: bool,
}

impl ClassCallable {
    fn from_initializer(
        initializer: &InitializerDecl<Native>,
        symbols: &class::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = initializer
            .callable()
            .params()
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            receiver: false,
            python_name: Name::new(initializer.name()).function(),
            native_name: symbols.initializer(initializer.name()),
            arguments: Self::arguments(None, &parameters),
            return_annotation: symbols.class_name().to_owned(),
            parameters,
            returns_value: true,
            return_class: symbols.class_name().to_owned(),
            wraps_return_handle: true,
        })
    }

    fn from_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        symbols: &class::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let receiver = method.callable().receiver().is_some();
        let parameters = method
            .callable()
            .params()
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect::<Result<Vec<_>>>()?;
        let return_class = match method.callable().returns().plan() {
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Some(package.class_name(class_id)?),
            ReturnPlan::HandleViaReturnSlot { .. } => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unsupported class method handle return",
                });
            }
            _ => None,
        };
        let return_annotation =
            PythonTypeHint::from_return(method.callable().returns().plan(), package)?.into_string();
        let wraps_return_handle = return_class.is_some();
        Ok(Self {
            receiver,
            python_name: Name::new(method.name()).function(),
            native_name: symbols.method(method.name()),
            arguments: Self::arguments(receiver.then_some("self._handle"), &parameters),
            parameters,
            return_annotation,
            returns_value: !matches!(method.callable().returns().plan(), ReturnPlan::Void),
            return_class: return_class.unwrap_or_default(),
            wraps_return_handle,
        })
    }

    fn arguments(receiver: Option<&str>, parameters: &[ParameterStub]) -> String {
        receiver
            .into_iter()
            .map(str::to_owned)
            .chain(
                parameters
                    .iter()
                    .map(|parameter| parameter.argument.clone()),
            )
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParameterStub {
    name: String,
    annotation: String,
    argument: String,
}

impl ParameterStub {
    fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure parameter stub",
            });
        };
        Ok(Self {
            name: Name::new(parameter.name()).function(),
            annotation: PythonTypeHint::from_parameter(plan, package)?.into_string(),
            argument: Self::argument(plan, parameter.name()),
        })
    }

    fn argument(
        plan: &ParamPlan<Native, IntoRust>,
        name: &boltffi_binding::CanonicalName,
    ) -> String {
        let name = Name::new(name).function();
        match plan {
            ParamPlan::Handle {
                target: HandleTarget::Class(_),
                presence: HandlePresence::Required,
                ..
            } => format!("{name}._handle"),
            _ => name,
        }
    }
}

struct PythonTypeHint {
    annotation: String,
}

impl PythonTypeHint {
    fn from_type_ref(ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        match ty {
            TypeRef::Primitive(primitive) => Self::from_primitive(*primitive),
            TypeRef::Record(record) => Ok(Self {
                annotation: package.record_name(*record)?,
            }),
            TypeRef::Enum(enumeration) => Ok(Self {
                annotation: package.enum_name(*enumeration)?,
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported stream item annotation",
            }),
        }
    }

    fn from_parameter(
        plan: &ParamPlan<Native, IntoRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match plan {
            ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                ..
            } => Self::from_primitive(*primitive),
            ParamPlan::Direct {
                ty: TypeRef::Record(record),
                ..
            } => Ok(Self {
                annotation: package.record_name(*record)?,
            }),
            ParamPlan::Direct {
                ty: TypeRef::Enum(enumeration),
                ..
            } => Ok(Self {
                annotation: package.enum_name(*enumeration)?,
            }),
            ParamPlan::Handle {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Ok(Self {
                annotation: package.class_name(class_id)?,
            }),
            ParamPlan::Handle {
                target: HandleTarget::Callback(_),
                ..
            } => Ok(Self::new("object")),
            ParamPlan::Encoded {
                ty: TypeRef::String,
                shape: native::BufferShape::Slice,
                ..
            } => Ok(Self::new("str")),
            ParamPlan::Encoded {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Slice,
                ..
            } => Ok(Self::new("bytes")),
            ParamPlan::Direct { .. }
            | ParamPlan::Encoded { .. }
            | ParamPlan::Handle { .. }
            | ParamPlan::ScalarOption { .. }
            | ParamPlan::DirectVec { .. } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported parameter stub",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown parameter stub",
            }),
        }
    }

    fn from_return(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self::new("None")),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => Self::from_primitive(*primitive),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Record(record),
            } => Ok(Self {
                annotation: package.record_name(*record)?,
            }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Enum(enumeration),
            } => Ok(Self {
                annotation: package.enum_name(*enumeration)?,
            }),
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Ok(Self {
                annotation: package.class_name(class_id)?,
            }),
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Callback(_),
                ..
            } => Ok(Self::new("object")),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::new("str")),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::new("bytes")),
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
                shape: "unsupported return stub",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown return stub",
            }),
        }
    }

    fn into_string(self) -> String {
        self.annotation
    }

    fn new(annotation: impl Into<String>) -> Self {
        Self {
            annotation: annotation.into(),
        }
    }

    fn from_primitive(primitive: Primitive) -> Result<Self> {
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
}

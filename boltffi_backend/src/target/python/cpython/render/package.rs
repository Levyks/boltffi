use std::{collections::HashMap, path::PathBuf};

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    Bindings, BuiltinType, CStyleEnumDecl, CanonicalName, ClassDecl, ClassId, ConstantDecl,
    ConstantValueDecl, CustomTypeId, DataEnumDecl, DataVariantDecl, DataVariantPayload,
    DeclarationRef, DefaultValue, DirectFieldDecl, DirectRecordDecl, EncodedFieldDecl,
    EncodedRecordDecl, EnumDecl, EnumId, ErrorDecl, ExecutionDecl, ExportedCallable,
    ExportedMethodDecl, FieldKey, FunctionDecl, HandlePresence, HandleTarget, IncomingParam,
    InitializerDecl, IntoRust, Native, NativeSymbol, OutOfRust, ParamDecl, ParamPlan, Primitive,
    Receive, RecordDecl, RecordId, ReturnPlan, StreamDecl, StreamItemPlan, TypeRef, native,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, FilePath, GeneratedFile, GeneratedOutput, Result},
    target::python::{
        cpython::render::{class, custom, enumeration, function, primitive, record, stream},
        name_style::{Name, PackageModule},
    },
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.py", escape = "none")]
struct InitTemplate {
    module_name_literal: String,
    package_name_literal: String,
    package_version_literal: String,
    library_name: String,
    uses_sequence_annotations: bool,
    uses_callable_annotations: bool,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
    has_data_enums: bool,
    records: Vec<RecordClass>,
    enums: Vec<EnumClass>,
    classes: Vec<Class>,
    constants: Vec<ConstantStub>,
    functions: Vec<FunctionStub>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.pyi", escape = "none")]
struct StubTemplate {
    uses_sequence_annotations: bool,
    uses_callable_annotations: bool,
    has_data_enums: bool,
    records: Vec<RecordClass>,
    enums: Vec<EnumClass>,
    classes: Vec<Class>,
    constants: Vec<ConstantStub>,
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
    module: PackageModule,
    distribution: String,
    version: Option<String>,
    library: String,
}

impl<'binding, 'bridge> Package<'binding, 'bridge> {
    pub fn new(
        bindings: &'binding Bindings<Native>,
        bridge: &'bridge PythonCExtBridgeContract,
        module: PackageModule,
        distribution: String,
        version: Option<String>,
        library: String,
    ) -> Self {
        Self {
            bindings,
            bridge,
            module,
            distribution,
            version,
            library,
        }
    }

    pub fn render(self) -> Result<GeneratedOutput> {
        let module = self.module_name();
        let package = self.distribution.clone();
        let version = self.version.clone();
        let records = self.records()?;
        let enums = self.enums()?;
        let classes = self.classes()?;
        let constants = self.constants()?;
        let functions = self.functions();
        let stubs = functions
            .iter()
            .map(|function| FunctionStub::from_declaration(function, &self))
            .collect::<Result<Vec<_>>>()?;
        self.validate_names(&records, &enums, &classes, &constants, &stubs)?;
        let uses_sequence_annotations = records.iter().any(RecordClass::uses_sequence_annotations)
            || enums.iter().any(EnumClass::uses_sequence_annotations)
            || classes.iter().any(Class::uses_sequence_annotations)
            || stubs.iter().any(FunctionStub::uses_sequence_annotations);
        let uses_callable_annotations = records.iter().any(RecordClass::uses_callable_annotations)
            || enums.iter().any(EnumClass::uses_callable_annotations)
            || classes.iter().any(Class::uses_callable_annotations)
            || stubs.iter().any(FunctionStub::uses_callable_annotations);
        let uses_wire_helpers = records.iter().any(RecordClass::has_wire)
            || records.iter().any(RecordClass::uses_wire_helpers)
            || enums.iter().any(EnumClass::has_wire)
            || enums.iter().any(EnumClass::uses_wire_helpers)
            || classes.iter().any(Class::uses_wire_helpers)
            || constants.iter().any(ConstantStub::uses_wire_helpers)
            || stubs.iter().any(FunctionStub::uses_wire_helpers);
        let uses_async_helpers = records.iter().any(RecordClass::uses_async_helpers)
            || enums.iter().any(EnumClass::uses_async_helpers)
            || classes.iter().any(Class::uses_async_helpers)
            || stubs.iter().any(FunctionStub::uses_async_helpers);
        Ok(GeneratedOutput::new(
            vec![
                self.file("pyproject.toml", Self::text(PyprojectTemplate.render()?))?,
                self.file(
                    "setup.py",
                    Self::text(
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
                    ),
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.py"),
                    Self::text(
                        InitTemplate {
                            module_name_literal: Self::literal(&module),
                            package_name_literal: Self::literal(&package),
                            package_version_literal: version
                                .as_deref()
                                .map(Self::literal)
                                .unwrap_or_else(|| "None".to_owned()),
                            library_name: self.library.clone(),
                            uses_sequence_annotations,
                            uses_callable_annotations,
                            uses_wire_helpers,
                            uses_async_helpers,
                            has_data_enums: enums.iter().any(EnumClass::has_wire),
                            records: records.clone(),
                            enums: enums.clone(),
                            classes: classes.clone(),
                            constants: constants.clone(),
                            functions: stubs.clone(),
                        }
                        .render()?,
                    ),
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.pyi"),
                    Self::text(
                        StubTemplate {
                            uses_sequence_annotations,
                            uses_callable_annotations,
                            has_data_enums: enums.iter().any(EnumClass::has_wire),
                            records,
                            enums,
                            classes,
                            constants,
                            functions: stubs,
                        }
                        .render()?,
                    ),
                )?,
                self.file(PathBuf::from(&module).join("py.typed"), String::new())?,
            ],
            Vec::new(),
        ))
    }

    fn module_name(&self) -> String {
        self.module.as_str().to_owned()
    }

    fn functions(&self) -> Vec<&'binding FunctionDecl<Native>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Function(function)
                    if function::Function::can_render(function.callable()) =>
                {
                    Some(function)
                }
                DeclarationRef::Function(_)
                | DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::Constant(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .collect()
    }

    fn constants(&self) -> Result<Vec<ConstantStub>> {
        self.bindings
            .decls()
            .iter()
            .filter_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Constant(constant) => Some(constant),
                DeclarationRef::Function(_)
                | DeclarationRef::Record(_)
                | DeclarationRef::Enum(_)
                | DeclarationRef::Class(_)
                | DeclarationRef::Callback(_)
                | DeclarationRef::Stream(_)
                | DeclarationRef::CustomType(_) => None,
            })
            .map(|constant| ConstantStub::from_declaration(constant, self))
            .collect()
    }

    fn records(&self) -> Result<Vec<RecordClass>> {
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
                RecordDecl::Direct(record) => RecordClass::from_direct(record, self),
                RecordDecl::Encoded(record) => RecordClass::from_encoded(record, self),
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
                EnumDecl::CStyle(enumeration) => EnumClass::from_c_style(enumeration, self),
                EnumDecl::Data(enumeration) => EnumClass::from_data(enumeration, self),
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
                    Some(record.name())
                }
                DeclarationRef::Record(RecordDecl::Encoded(record)) if record.id() == record_id => {
                    Some(record.name())
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
            .map(|name| Name::new(name).class())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "record type hint without declaration",
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
                    Some(enumeration.name())
                }
                DeclarationRef::Enum(EnumDecl::Data(enumeration))
                    if enumeration.id() == enum_id =>
                {
                    Some(enumeration.name())
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
            .map(|name| Name::new(name).class())
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum type hint without declaration",
            })
    }

    fn enum_variant_expression(
        &self,
        enum_name: &CanonicalName,
        variant_name: &CanonicalName,
    ) -> Result<String> {
        self.bindings
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration))
                    if enumeration.name() == enum_name =>
                {
                    Some(EnumVariantStyle::CStyle)
                }
                DeclarationRef::Enum(EnumDecl::Data(enumeration))
                    if enumeration.name() == enum_name =>
                {
                    Some(EnumVariantStyle::Data)
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
            .map(|style| style.expression(enum_name, variant_name))
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum constant without declaration",
            })
    }

    fn enum_wire(&self, enum_id: EnumId) -> Result<EnumWire> {
        self.bindings
            .decls()
            .iter()
            .find_map(|decl| match DeclarationRef::from(decl) {
                DeclarationRef::Enum(EnumDecl::CStyle(enumeration))
                    if enumeration.id() == enum_id =>
                {
                    Some(EnumWire::CStyle(enumeration.repr().primitive()))
                }
                DeclarationRef::Enum(EnumDecl::Data(enumeration))
                    if enumeration.id() == enum_id =>
                {
                    Some(EnumWire::Data {
                        class_name: Name::new(enumeration.name()).class(),
                    })
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
            .ok_or(Error::UnsupportedTarget {
                target: "python",
                shape: "enum wire type without declaration",
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

    fn custom_representation(&self, custom_type: CustomTypeId) -> Result<&'binding TypeRef> {
        custom::CustomTypes::new(self.bindings, "python").representation(custom_type)
    }

    fn file(&self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Result<GeneratedFile> {
        Ok(GeneratedFile::new(FilePath::new(path.into())?, contents))
    }

    fn literal(value: impl AsRef<str>) -> String {
        format!("{:?}", value.as_ref())
    }

    fn text(contents: String) -> String {
        match contents.ends_with('\n') {
            true => contents,
            false => format!("{contents}\n"),
        }
    }

    fn validate_names(
        &self,
        records: &[RecordClass],
        enums: &[EnumClass],
        classes: &[Class],
        constants: &[ConstantStub],
        functions: &[FunctionStub],
    ) -> Result<()> {
        let scope = match enums.iter().any(EnumClass::is_int_enum) {
            true => {
                NameScope::new("python module").insert("IntEnum", "imported enum base `IntEnum`")?
            }
            false => NameScope::new("python module"),
        };
        scope
            .insert_all(records.iter().map(|record| record.top_level_name()))
            .and_then(|scope| scope.insert_all(enums.iter().map(EnumClass::top_level_name)))
            .and_then(|scope| {
                scope.insert_all(enums.iter().flat_map(EnumClass::data_variant_names))
            })
            .and_then(|scope| scope.insert_all(classes.iter().map(Class::top_level_name)))
            .and_then(|scope| scope.insert_all(classes.iter().flat_map(Class::subscription_names)))
            .and_then(|scope| scope.insert_all(constants.iter().map(ConstantStub::top_level_name)))
            .and_then(|scope| scope.insert_all(functions.iter().map(FunctionStub::top_level_name)))
            .map(|_| ())?;

        records.iter().try_for_each(RecordClass::validate_names)?;
        enums.iter().try_for_each(EnumClass::validate_names)?;
        classes.iter().try_for_each(Class::validate_names)?;
        functions.iter().try_for_each(FunctionStub::validate_names)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NameScope {
    label: String,
    names: HashMap<String, String>,
}

impl NameScope {
    fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            names: HashMap::new(),
        }
    }

    fn insert(mut self, name: impl Into<String>, subject: impl Into<String>) -> Result<Self> {
        let name = name.into();
        let subject = subject.into();
        if let Some(existing) = self.names.insert(name.clone(), subject.clone()) {
            return Err(Error::PythonNameCollision {
                scope: self.label,
                name,
                existing,
                colliding: subject,
            });
        }
        Ok(self)
    }

    fn insert_all<I>(self, names: I) -> Result<Self>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        names
            .into_iter()
            .try_fold(self, |scope, (name, subject)| scope.insert(name, subject))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FunctionStub {
    python_name: String,
    parameters: Vec<ParameterStub>,
    return_annotation: String,
    asynchronous: bool,
    body: Vec<String>,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
}

impl FunctionStub {
    fn from_declaration(
        function: &FunctionDecl<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = function
            .callable()
            .params()
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect::<Result<Vec<_>>>()?;
        let arguments = parameters
            .iter()
            .map(|parameter| parameter.argument.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let returned = ReturnStub::from_callable(function.callable(), package)?;
        let native_name = Name::new(function.name()).function();
        let native_call = format!("_native.{}({arguments})", native_name);
        let body = CallableBody::from_callable(
            function.callable(),
            &native_name,
            native_call,
            &returned.value,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers)
            || returned.value.uses_wire_helpers();
        Ok(Self {
            python_name: Name::new(function.name()).function(),
            parameters,
            return_annotation: returned.annotation,
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            uses_wire_helpers,
        })
    }

    fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    fn uses_async_helpers(&self) -> bool {
        self.uses_async_helpers
    }

    fn uses_sequence_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_sequence_annotation)
    }

    fn uses_callable_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_callable_annotation)
    }

    fn validate_names(&self) -> Result<()> {
        Self::parameter_scope(format!("function `{}`", self.python_name), &self.parameters)
            .map(|_| ())
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("function `{}`", self.python_name),
        )
    }

    fn parameter_scope(
        label: impl Into<String>,
        parameters: &[ParameterStub],
    ) -> Result<NameScope> {
        NameScope::new(label).insert_all(parameters.iter().map(ParameterStub::parameter_name))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConstantStub {
    python_name: String,
    annotation: String,
    expression: String,
    uses_wire_helpers: bool,
}

impl ConstantStub {
    fn from_declaration(
        constant: &ConstantDecl<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match constant.value() {
            ConstantValueDecl::Inline { ty, value, .. } => {
                Self::from_inline(constant, ty, value, package)
            }
            ConstantValueDecl::Accessor { callable, .. } => {
                let returned = ReturnStub::from_plan(callable.returns().plan(), package)?;
                let native_call = format!("_native.{}()", Name::new(constant.name()).function());
                Ok(Self {
                    python_name: Name::new(constant.name()).function(),
                    annotation: returned.annotation,
                    expression: returned.value.expression(native_call),
                    uses_wire_helpers: returned.value.uses_wire_helpers(),
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown constant value package",
            }),
        }
    }

    fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("constant `{}`", self.python_name),
        )
    }

    fn from_inline(
        constant: &ConstantDecl<Native>,
        ty: &TypeRef,
        value: &DefaultValue,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        Ok(Self {
            python_name: Name::new(constant.name()).function(),
            annotation: PythonTypeHint::from_type_ref(ty, package)?.into_string(),
            expression: ConstantExpression::new(value, package)?.into_string(),
            uses_wire_helpers: false,
        })
    }
}

struct ConstantExpression {
    expression: String,
}

impl ConstantExpression {
    fn new(value: &DefaultValue, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            expression: match value {
                DefaultValue::Bool(value) => Self::bool(*value),
                DefaultValue::Integer(value) => value.get().to_string(),
                DefaultValue::Float(value) => Self::float(value.to_f64()),
                DefaultValue::String(value) => Package::literal(value),
                DefaultValue::EnumVariant {
                    enum_name,
                    variant_name,
                } => package.enum_variant_expression(enum_name, variant_name)?,
                DefaultValue::Null => "None".to_owned(),
                _ => {
                    return Err(Error::UnsupportedTarget {
                        target: "python",
                        shape: "unknown constant literal",
                    });
                }
            },
        })
    }

    fn into_string(self) -> String {
        self.expression
    }

    fn bool(value: bool) -> String {
        match value {
            true => "True".to_owned(),
            false => "False".to_owned(),
        }
    }

    fn float(value: f64) -> String {
        if value.is_nan() {
            return "float(\"nan\")".to_owned();
        }
        if value == f64::INFINITY {
            return "float(\"inf\")".to_owned();
        }
        if value == f64::NEG_INFINITY {
            return "float(\"-inf\")".to_owned();
        }
        if value == 0.0 && value.is_sign_negative() {
            return "-0.0".to_owned();
        }
        value.to_string()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RecordClass {
    class_name: String,
    register_method: String,
    fields: Vec<RecordField>,
    wire: Option<EncodedRecordWire>,
    constructors: Vec<AssociatedCallable>,
    static_methods: Vec<AssociatedCallable>,
    instance_methods: Vec<AssociatedCallable>,
}

impl RecordClass {
    fn from_direct(record: &DirectRecordDecl<Native>, package: &Package<'_, '_>) -> Result<Self> {
        let c_record =
            package
                .bridge
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
            wire: None,
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    fn from_encoded(record: &EncodedRecordDecl<Native>, package: &Package<'_, '_>) -> Result<Self> {
        let symbols = record::Symbols::from_encoded(record)?;
        let fields = record
            .fields()
            .iter()
            .map(|field| RecordField::from_encoded(field, package))
            .collect::<Result<Vec<_>>>()?;
        let wire_fields = record
            .fields()
            .iter()
            .map(|field| EncodedRecordField::from_field(field, package))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            class_name: symbols.class_name().to_owned(),
            register_method: symbols.register_method().to_owned(),
            fields,
            wire: Some(EncodedRecordWire {
                fields: wire_fields,
            }),
            constructors: Self::constructors(record.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(record.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(record.methods(), &symbols, package)?,
        })
    }

    fn has_wire(&self) -> bool {
        self.wire.is_some()
    }

    fn uses_wire_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_wire_helpers)
    }

    fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("record `{}`", self.class_name))
            .insert_all(self.fields.iter().map(RecordField::field_name))
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .map(|_| ())?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.clone(),
            format!("record `{}`", self.class_name),
        )
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn constructors(
        initializers: &[InitializerDecl<Native>],
        symbols: &record::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        initializers
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_value_initializer(
                    initializer,
                    symbols.initializer(initializer.name()),
                    package,
                )
            })
            .collect()
    }

    fn static_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_none()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name()),
                    None,
                    None,
                    package,
                )
            })
            .collect()
    }

    fn instance_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &record::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_some()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name()),
                    Some("self"),
                    method
                        .callable()
                        .receiver()
                        .filter(|receiver| matches!(receiver, Receive::ByMutRef))
                        .map(|_| symbols.class_name()),
                    package,
                )
            })
            .collect()
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

    fn from_encoded(field: &EncodedFieldDecl, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            name: Self::name(field.key())?,
            annotation: PythonTypeHint::from_type_ref(field.ty(), package)?.into_string(),
        })
    }

    fn field_name(&self) -> (String, String) {
        (self.name.clone(), format!("field `{}`", self.name))
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
struct EncodedRecordWire {
    fields: Vec<EncodedRecordField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EncodedRecordField {
    name: String,
    encode: String,
    decode: String,
}

impl EncodedRecordField {
    fn from_field(field: &EncodedFieldDecl, package: &Package<'_, '_>) -> Result<Self> {
        let name = RecordField::name(field.key())?;
        Ok(Self {
            encode: WireValue::new(format!("self.{name}"), field.ty(), package)?.encode(),
            decode: WireValue::reader(field.ty(), package)?.decode(),
            name,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WireValue {
    expression: String,
}

impl WireValue {
    fn new(value: impl Into<String>, ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            expression: Self::encode_expr(value.into(), ty, package)?,
        })
    }

    fn reader(ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            expression: Self::decode_expr(ty, package)?,
        })
    }

    fn encode(self) -> String {
        self.expression
    }

    fn decode(self) -> String {
        self.expression
    }

    fn encode_expr(value: String, ty: &TypeRef, package: &Package<'_, '_>) -> Result<String> {
        match ty {
            TypeRef::Primitive(primitive) => {
                let stem = primitive::Runtime::new(*primitive).wire_stem()?;
                Ok(format!("_boltffi_wire_{stem}({value})"))
            }
            TypeRef::String => Ok(format!("_boltffi_wire_string({value})")),
            TypeRef::Bytes => Ok(format!("_boltffi_wire_bytes({value})")),
            TypeRef::Builtin(builtin) => Ok(Self::encode_builtin(value, *builtin)),
            TypeRef::Custom(custom_type) => {
                Self::encode_expr(value, package.custom_representation(*custom_type)?, package)
            }
            TypeRef::Record(_) => Ok(format!("{value}._boltffi_wire()")),
            TypeRef::Enum(enumeration) => match package.enum_wire(*enumeration)? {
                EnumWire::CStyle(primitive) => {
                    let stem = primitive::Runtime::new(primitive).wire_stem()?;
                    let enum_name = package.enum_name(*enumeration)?;
                    let enum_name_literal = Package::literal(&enum_name);
                    Ok(format!(
                        "_boltffi_wire_{stem}(_boltffi_enum_value({value}, {enum_name}, {enum_name_literal}))"
                    ))
                }
                EnumWire::Data { .. } => Ok(format!("{value}._boltffi_wire()")),
            },
            TypeRef::Optional(inner) => {
                let inner = Self::encode_expr("value".to_owned(), inner, package)?;
                Ok(format!(
                    "_boltffi_wire_optional({value}, lambda value: {inner})"
                ))
            }
            TypeRef::Result { ok, err } => {
                let ok = Self::encode_expr("payload".to_owned(), ok, package)?;
                let err = Self::encode_expr("payload".to_owned(), err, package)?;
                Ok(format!(
                    "_boltffi_wire_result({value}, lambda payload: {ok}, lambda payload: {err})"
                ))
            }
            TypeRef::Sequence(element) => {
                let element = Self::encode_expr("value".to_owned(), element, package)?;
                Ok(format!(
                    "_boltffi_wire_sequence({value}, lambda value: {element})"
                ))
            }
            TypeRef::Tuple(elements) => {
                let fields = elements
                    .iter()
                    .enumerate()
                    .map(|(index, element)| {
                        Self::encode_expr(format!("{value}[{index}]"), element, package)
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("b\"\".join(({fields},))"))
            }
            TypeRef::Map { key, value: item } => {
                let key = Self::encode_expr("key".to_owned(), key, package)?;
                let item = Self::encode_expr("item".to_owned(), item, package)?;
                Ok(format!(
                    "_boltffi_wire_map({value}, lambda key: {key}, lambda item: {item})"
                ))
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported encoded record field",
            }),
        }
    }

    fn decode_expr(ty: &TypeRef, package: &Package<'_, '_>) -> Result<String> {
        match ty {
            TypeRef::Primitive(primitive) => {
                let stem = primitive::Runtime::new(*primitive).wire_stem()?;
                Ok(format!("reader.{stem}()"))
            }
            TypeRef::String => Ok("reader.string()".to_owned()),
            TypeRef::Bytes => Ok("reader.bytes()".to_owned()),
            TypeRef::Builtin(builtin) => Ok(Self::decode_builtin(*builtin)),
            TypeRef::Custom(custom_type) => {
                Self::decode_expr(package.custom_representation(*custom_type)?, package)
            }
            TypeRef::Record(record) => Ok(format!(
                "{}._boltffi_from_reader(reader)",
                package.record_name(*record)?
            )),
            TypeRef::Enum(enumeration) => match package.enum_wire(*enumeration)? {
                EnumWire::CStyle(primitive) => {
                    let stem = primitive::Runtime::new(primitive).wire_stem()?;
                    Ok(format!(
                        "{}(reader.{stem}())",
                        package.enum_name(*enumeration)?
                    ))
                }
                EnumWire::Data { class_name } => {
                    Ok(format!("{class_name}._boltffi_from_reader(reader)"))
                }
            },
            TypeRef::Optional(inner) => {
                let inner = Self::decode_expr(inner, package)?;
                Ok(format!("reader.optional(lambda: {inner})"))
            }
            TypeRef::Result { ok, err } => {
                let ok = Self::decode_expr(ok, package)?;
                let err = Self::decode_expr(err, package)?;
                Ok(format!("reader.result(lambda: {ok}, lambda: {err})"))
            }
            TypeRef::Sequence(element) => {
                let element = Self::decode_expr(element, package)?;
                Ok(format!("reader.sequence(lambda: {element})"))
            }
            TypeRef::Tuple(elements) => {
                let fields = elements
                    .iter()
                    .map(|element| Self::decode_expr(element, package))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("({fields},)"))
            }
            TypeRef::Map { key, value } => {
                let key = Self::decode_expr(key, package)?;
                let value = Self::decode_expr(value, package)?;
                Ok(format!("reader.map(lambda: {key}, lambda: {value})"))
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unsupported encoded record field",
            }),
        }
    }

    fn encode_builtin(value: String, builtin: BuiltinType) -> String {
        match builtin {
            BuiltinType::Duration => format!("_boltffi_wire_duration({value})"),
            BuiltinType::SystemTime => format!("_boltffi_wire_system_time({value})"),
            BuiltinType::Uuid => format!("_boltffi_wire_uuid({value})"),
            BuiltinType::Url => format!("_boltffi_wire_url({value})"),
        }
    }

    fn decode_builtin(builtin: BuiltinType) -> String {
        match builtin {
            BuiltinType::Duration => "reader.duration()".to_owned(),
            BuiltinType::SystemTime => "reader.system_time()".to_owned(),
            BuiltinType::Uuid => "reader.uuid()".to_owned(),
            BuiltinType::Url => "reader.url()".to_owned(),
        }
    }
}

enum EnumWire {
    CStyle(Primitive),
    Data { class_name: String },
}

enum EnumVariantStyle {
    CStyle,
    Data,
}

impl EnumVariantStyle {
    fn expression(self, enum_name: &CanonicalName, variant_name: &CanonicalName) -> String {
        match self {
            Self::CStyle => format!(
                "{}.{}",
                Name::new(enum_name).class(),
                Name::new(variant_name).enum_member()
            ),
            Self::Data => format!(
                "{}{}()",
                Name::new(enum_name).class(),
                Name::new(variant_name).class()
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct EnumClass {
    class_name: String,
    register_method: String,
    variants: Vec<EnumVariant>,
    wire: Option<DataEnumWire>,
    constructors: Vec<AssociatedCallable>,
    static_methods: Vec<AssociatedCallable>,
    instance_methods: Vec<AssociatedCallable>,
}

impl EnumClass {
    fn from_c_style(
        enumeration: &CStyleEnumDecl<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let class = enumeration::PythonClass::from_c_style(enumeration, package.bridge)?;
        let c_enum = package.bridge.source_c_style_enum(enumeration.id()).ok_or(
            Error::UnsupportedTarget {
                target: "python",
                shape: "c-style enum package without C typedef",
            },
        )?;
        let symbols = enumeration::Symbols::from_c_style(enumeration, c_enum)?;
        Ok(Self {
            class_name: class.class_name().to_owned(),
            register_method: class.register_method().to_owned(),
            variants: class
                .variants()
                .iter()
                .map(EnumVariant::from_variant)
                .collect(),
            wire: None,
            constructors: Self::constructors(enumeration.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(enumeration.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(enumeration.methods(), &symbols, package)?,
        })
    }

    fn from_data(enumeration: &DataEnumDecl<Native>, package: &Package<'_, '_>) -> Result<Self> {
        let symbols = enumeration::Symbols::from_data(enumeration)?;
        let class_name = symbols.class_name().to_owned();
        Ok(Self {
            class_name: class_name.clone(),
            register_method: symbols.register_method().to_owned(),
            variants: Vec::new(),
            wire: Some(DataEnumWire {
                variants: enumeration
                    .variants()
                    .iter()
                    .map(|variant| DataEnumVariant::from_variant(variant, &class_name, package))
                    .collect::<Result<Vec<_>>>()?,
            }),
            constructors: Self::constructors(enumeration.initializers(), &symbols, package)?,
            static_methods: Self::static_methods(enumeration.methods(), &symbols, package)?,
            instance_methods: Self::instance_methods(enumeration.methods(), &symbols, package)?,
        })
    }

    fn has_wire(&self) -> bool {
        self.wire.is_some()
    }

    fn uses_wire_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_wire_helpers)
    }

    fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.constructors
            .iter()
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn validate_names(&self) -> Result<()> {
        let scope = match self.is_int_enum() {
            true => NameScope::new(format!("enum `{}`", self.class_name))
                .insert("name", "reserved IntEnum property `name`")?
                .insert("value", "reserved IntEnum property `value`")?,
            false => NameScope::new(format!("enum `{}`", self.class_name)),
        };
        scope
            .insert_all(self.variants.iter().map(EnumVariant::member_name))
            .and_then(|scope| {
                scope.insert_all(self.callables().map(AssociatedCallable::member_name))
            })
            .map(|_| ())?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))?;
        self.wire
            .iter()
            .flat_map(|wire| wire.variants.iter())
            .try_for_each(DataEnumVariant::validate_names)
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.clone(),
            format!("enum `{}`", self.class_name),
        )
    }

    fn data_variant_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.wire
            .iter()
            .flat_map(|wire| wire.variants.iter())
            .map(DataEnumVariant::top_level_name)
    }

    fn is_int_enum(&self) -> bool {
        self.wire.is_none()
    }

    fn constructors(
        initializers: &[InitializerDecl<Native>],
        symbols: &enumeration::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        initializers
            .iter()
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_value_initializer(
                    initializer,
                    symbols.initializer(initializer.name()),
                    package,
                )
            })
            .collect()
    }

    fn static_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &enumeration::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_none()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name()),
                    None,
                    None,
                    package,
                )
            })
            .collect()
    }

    fn instance_methods(
        methods: &[ExportedMethodDecl<Native, NativeSymbol>],
        symbols: &enumeration::Symbols,
        package: &Package<'_, '_>,
    ) -> Result<Vec<AssociatedCallable>> {
        methods
            .iter()
            .filter(|method| {
                function::Function::can_render(method.callable())
                    && method.callable().receiver().is_some()
            })
            .map(|method| {
                AssociatedCallable::from_value_method(
                    method,
                    symbols.method(method.name()),
                    Some("self"),
                    method
                        .callable()
                        .receiver()
                        .filter(|receiver| matches!(receiver, Receive::ByMutRef))
                        .map(|_| symbols.class_name()),
                    package,
                )
            })
            .collect()
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

    fn member_name(&self) -> (String, String) {
        (self.name.clone(), format!("enum member `{}`", self.name))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataEnumWire {
    variants: Vec<DataEnumVariant>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DataEnumVariant {
    class_name: String,
    tag: u32,
    fields: Vec<RecordField>,
    wire_fields: Vec<EncodedRecordField>,
}

impl DataEnumVariant {
    fn from_variant(
        variant: &DataVariantDecl,
        enum_class_name: &str,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let fields = Self::payload_fields(variant.payload())?;
        Ok(Self {
            class_name: format!("{}{}", enum_class_name, Name::new(variant.name()).class()),
            tag: variant.tag().get(),
            fields: fields
                .iter()
                .map(|field| RecordField::from_encoded(field, package))
                .collect::<Result<Vec<_>>>()?,
            wire_fields: fields
                .iter()
                .map(|field| EncodedRecordField::from_field(field, package))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn has_fields(&self) -> bool {
        !self.fields.is_empty()
    }

    fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("data enum variant `{}`", self.class_name))
            .insert_all(self.fields.iter().map(RecordField::field_name))
            .map(|_| ())
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.clone(),
            format!("data enum variant `{}`", self.class_name),
        )
    }

    fn payload_fields(payload: &DataVariantPayload) -> Result<&[EncodedFieldDecl]> {
        Ok(match payload {
            DataVariantPayload::Unit => &[],
            DataVariantPayload::Tuple(fields) | DataVariantPayload::Struct(fields) => fields,
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unknown data enum payload",
                });
            }
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Class {
    class_name: String,
    release_method: String,
    init: Vec<AssociatedCallable>,
    constructors: Vec<AssociatedCallable>,
    static_methods: Vec<AssociatedCallable>,
    instance_methods: Vec<AssociatedCallable>,
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
            .filter(|initializer| function::Function::can_render(initializer.callable()))
            .map(|initializer| {
                AssociatedCallable::from_class_initializer(initializer, &symbols, package)
            })
            .collect::<Result<Vec<_>>>()?;
        let init = constructors
            .iter()
            .filter(|constructor| constructor.python_name == "new")
            .cloned()
            .collect::<Vec<_>>();
        if init.iter().any(AssociatedCallable::is_async) {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "async __init__",
            });
        }
        let constructors = constructors
            .into_iter()
            .filter(|constructor| constructor.python_name != "new")
            .collect::<Vec<_>>();
        let methods = declaration
            .methods()
            .iter()
            .filter(|method| function::Function::can_render(method.callable()))
            .map(|method| AssociatedCallable::from_class_method(method, &symbols, package))
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

    fn uses_wire_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_wire_helpers)
            || self.streams.iter().any(ClassStream::uses_wire_helpers)
    }

    fn uses_async_helpers(&self) -> bool {
        self.callables().any(AssociatedCallable::uses_async_helpers)
    }

    fn uses_sequence_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_sequence_annotations)
    }

    fn uses_callable_annotations(&self) -> bool {
        self.callables()
            .any(AssociatedCallable::uses_callable_annotations)
    }

    fn callables(&self) -> impl Iterator<Item = &AssociatedCallable> {
        self.init
            .iter()
            .chain(&self.constructors)
            .chain(&self.static_methods)
            .chain(&self.instance_methods)
    }

    fn validate_names(&self) -> Result<()> {
        NameScope::new(format!("class `{}`", self.class_name))
            .insert_all(self.callables().map(AssociatedCallable::member_name))
            .and_then(|scope| scope.insert_all(self.streams.iter().map(ClassStream::member_name)))
            .map(|_| ())?;
        self.callables()
            .try_for_each(|callable| callable.validate_names(&self.class_name))
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.class_name.clone(),
            format!("class `{}`", self.class_name),
        )
    }

    fn subscription_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.streams.iter().map(ClassStream::top_level_name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClassStream {
    python_name: String,
    subscribe_method: String,
    subscription_class: String,
    item_annotation: String,
    pop_batch_method: String,
    pop_batch_body: Vec<String>,
    wait_method: String,
    unsubscribe_method: String,
    free_method: String,
    uses_wire_helpers: bool,
}

impl ClassStream {
    fn from_declaration(
        declaration: &StreamDecl<Native>,
        class_name: &str,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let symbols = stream::Symbols::new(declaration);
        let item = StreamItem::from_plan(declaration.item(), package)?;
        let pop_batch_body = item.pop_batch_body(symbols.pop_batch());
        let uses_wire_helpers = item.uses_wire_helpers;
        Ok(Self {
            python_name: Name::new(declaration.name()).function(),
            subscribe_method: symbols.subscribe(),
            subscription_class: format!(
                "{}{}Subscription",
                class_name,
                Name::new(declaration.name()).class()
            ),
            item_annotation: item.annotation,
            pop_batch_method: symbols.pop_batch(),
            pop_batch_body,
            wait_method: symbols.wait(),
            unsubscribe_method: symbols.unsubscribe(),
            free_method: symbols.free(),
            uses_wire_helpers,
        })
    }

    fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    fn member_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("stream `{}`", self.python_name),
        )
    }

    fn top_level_name(&self) -> (String, String) {
        (
            self.subscription_class.clone(),
            format!("stream subscription `{}`", self.subscription_class),
        )
    }
}

struct StreamItem {
    annotation: String,
    decode: Option<String>,
    uses_wire_helpers: bool,
}

impl StreamItem {
    fn from_plan(plan: &StreamItemPlan<Native>, package: &Package<'_, '_>) -> Result<Self> {
        match plan {
            StreamItemPlan::Direct { ty, .. } => Ok(Self {
                annotation: PythonTypeHint::from_type_ref(ty, package)?.into_string(),
                decode: None,
                uses_wire_helpers: false,
            }),
            StreamItemPlan::Encoded { ty, .. } => {
                let batch = TypeRef::Sequence(Box::new(ty.clone()));
                Ok(Self {
                    annotation: PythonTypeHint::from_type_ref(ty, package)?.into_string(),
                    decode: Some(WireValue::reader(&batch, package)?.decode()),
                    uses_wire_helpers: true,
                })
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown stream item package",
            }),
        }
    }

    fn pop_batch_body(&self, method: String) -> Vec<String> {
        match &self.decode {
            Some(decode) => vec![
                format!("data = _native.{method}(self._require_handle(), max_count)"),
                format!("return _boltffi_read_wire(data, lambda reader: {decode}) if data else []"),
            ],
            None => vec![format!(
                "return _native.{method}(self._require_handle(), max_count)"
            )],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AssociatedCallable {
    receiver: bool,
    python_name: String,
    native_name: String,
    parameters: Vec<ParameterStub>,
    arguments: String,
    return_annotation: String,
    asynchronous: bool,
    body: Vec<String>,
    uses_wire_helpers: bool,
    uses_async_helpers: bool,
}

impl AssociatedCallable {
    fn from_class_initializer(
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
        let arguments = Self::arguments(None, &parameters);
        let native_name = symbols.initializer(initializer.name());
        let native_call = format!("_native.{native_name}({arguments})");
        let returned = ReturnedValue::ClassHandle(symbols.class_name().to_owned());
        let body = CallableBody::from_callable(
            initializer.callable(),
            &native_name,
            native_call,
            &returned,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers);
        Ok(Self {
            receiver: false,
            python_name: Name::new(initializer.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            return_annotation: symbols.class_name().to_owned(),
            parameters,
            uses_wire_helpers,
        })
    }

    fn from_class_method(
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
        let returned = ReturnStub::from_callable(method.callable(), package)?;
        let arguments = Self::arguments(receiver.then_some("self._handle"), &parameters);
        let native_name = symbols.method(method.name());
        let native_call = format!("_native.{native_name}({arguments})");
        let body = CallableBody::from_callable(
            method.callable(),
            &native_name,
            native_call,
            &returned.value,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers)
            || returned.value.uses_wire_helpers();
        Ok(Self {
            receiver,
            python_name: Name::new(method.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            parameters,
            return_annotation: returned.annotation,
            uses_wire_helpers,
        })
    }

    fn from_value_initializer(
        initializer: &InitializerDecl<Native>,
        native_name: String,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = Self::parameters(initializer.callable().params(), package)?;
        let returned = ReturnStub::from_callable(initializer.callable(), package)?;
        let arguments = Self::arguments(None, &parameters);
        let native_call = format!("_native.{native_name}({arguments})");
        let body = CallableBody::from_callable(
            initializer.callable(),
            &native_name,
            native_call,
            &returned.value,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers)
            || returned.value.uses_wire_helpers();
        Ok(Self {
            receiver: false,
            python_name: Name::new(initializer.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            return_annotation: returned.annotation,
            parameters,
            uses_wire_helpers,
        })
    }

    fn from_value_method(
        method: &ExportedMethodDecl<Native, NativeSymbol>,
        native_name: String,
        receiver: Option<&str>,
        mutated_receiver_type: Option<&str>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let parameters = Self::parameters(method.callable().params(), package)?;
        let returned = match mutated_receiver_type {
            Some(annotation) => ReturnStub::native(annotation),
            None => ReturnStub::from_callable(method.callable(), package)?,
        };
        let arguments = Self::arguments(receiver, &parameters);
        let native_call = format!("_native.{native_name}({arguments})");
        let body = CallableBody::from_callable(
            method.callable(),
            &native_name,
            native_call,
            &returned.value,
        )?;
        let uses_wire_helpers = parameters.iter().any(ParameterStub::uses_wire_helpers)
            || returned.value.uses_wire_helpers();
        Ok(Self {
            receiver: receiver.is_some(),
            python_name: Name::new(method.name()).function(),
            asynchronous: body.is_async(),
            uses_async_helpers: body.uses_async_helpers(),
            body: body.into_lines(),
            native_name,
            arguments,
            parameters,
            return_annotation: returned.annotation,
            uses_wire_helpers,
        })
    }

    fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    fn uses_async_helpers(&self) -> bool {
        self.uses_async_helpers
    }

    fn is_async(&self) -> bool {
        self.asynchronous
    }

    fn uses_sequence_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_sequence_annotation)
    }

    fn uses_callable_annotations(&self) -> bool {
        self.parameters
            .iter()
            .any(ParameterStub::uses_callable_annotation)
    }

    fn validate_names(&self, owner: &str) -> Result<()> {
        FunctionStub::parameter_scope(
            format!("method `{}.{}`", owner, self.python_name),
            &self.parameters,
        )
        .map(|_| ())
    }

    fn member_name(&self) -> (String, String) {
        (
            self.python_name.clone(),
            format!("method `{}`", self.python_name),
        )
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

    fn parameters(
        parameters: &[ParamDecl<Native, IntoRust>],
        package: &Package<'_, '_>,
    ) -> Result<Vec<ParameterStub>> {
        parameters
            .iter()
            .map(|parameter| ParameterStub::from_declaration(parameter, package))
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParameterStub {
    name: String,
    annotation: String,
    argument: String,
    uses_sequence_annotation: bool,
    uses_callable_annotation: bool,
    uses_wire_helpers: bool,
}

impl ParameterStub {
    fn from_declaration(
        parameter: &ParamDecl<Native, IntoRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Ok(Self {
                name: Name::new(parameter.name()).function(),
                annotation: "Callable[..., object]".to_owned(),
                argument: Name::new(parameter.name()).function(),
                uses_sequence_annotation: false,
                uses_callable_annotation: true,
                uses_wire_helpers: false,
            });
        };
        let argument = Self::argument(plan, parameter.name(), package)?;
        let uses_wire_helpers = Self::uses_wire(plan, package)?;
        let annotation = PythonTypeHint::from_parameter(plan, package)?;
        Ok(Self {
            name: Name::new(parameter.name()).function(),
            uses_sequence_annotation: annotation.uses_sequence(),
            uses_callable_annotation: false,
            annotation: annotation.into_string(),
            argument,
            uses_wire_helpers,
        })
    }

    fn uses_wire_helpers(&self) -> bool {
        self.uses_wire_helpers
    }

    fn uses_sequence_annotation(&self) -> bool {
        self.uses_sequence_annotation
    }

    fn uses_callable_annotation(&self) -> bool {
        self.uses_callable_annotation
    }

    fn parameter_name(&self) -> (String, String) {
        (self.name.clone(), format!("parameter `{}`", self.name))
    }

    fn argument(
        plan: &ParamPlan<Native, IntoRust>,
        name: &boltffi_binding::CanonicalName,
        package: &Package<'_, '_>,
    ) -> Result<String> {
        let name = Name::new(name).function();
        Ok(match plan {
            ParamPlan::Handle {
                target: HandleTarget::Class(_),
                presence: HandlePresence::Required,
                ..
            } => format!("{name}._handle"),
            ParamPlan::Handle {
                target: HandleTarget::Class(_),
                presence: HandlePresence::Nullable,
                ..
            } => format!("(0 if {name} is None else {name}._handle)"),
            ParamPlan::Encoded { ty, .. } => Self::encoded_argument(name, ty, package)?,
            _ => name,
        })
    }

    fn encoded_argument(name: String, ty: &TypeRef, package: &Package<'_, '_>) -> Result<String> {
        let encoded_type = match ty {
            TypeRef::Custom(custom_type) => package.custom_representation(*custom_type)?,
            _ => ty,
        };
        Ok(match encoded_type {
            TypeRef::Primitive(_)
            | TypeRef::String
            | TypeRef::Bytes
            | TypeRef::Record(_)
            | TypeRef::Enum(_) => name,
            _ => WireValue::new(name, encoded_type, package)?.encode(),
        })
    }

    fn uses_wire(plan: &ParamPlan<Native, IntoRust>, package: &Package<'_, '_>) -> Result<bool> {
        let ParamPlan::Encoded {
            ty,
            shape: native::BufferShape::Slice,
            ..
        } = plan
        else {
            return Ok(false);
        };
        let encoded_type = match ty {
            TypeRef::Custom(custom_type) => package.custom_representation(*custom_type)?,
            _ => ty,
        };
        Ok(!matches!(
            encoded_type,
            TypeRef::Primitive(_)
                | TypeRef::String
                | TypeRef::Bytes
                | TypeRef::Record(_)
                | TypeRef::Enum(_)
        ))
    }
}

struct ReturnStub {
    annotation: String,
    value: ReturnedValue,
}

impl ReturnStub {
    fn native(annotation: impl Into<String>) -> Self {
        Self {
            annotation: annotation.into(),
            value: ReturnedValue::Native,
        }
    }

    fn from_callable(
        callable: &ExportedCallable<Native>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match callable.error() {
            ErrorDecl::None(_) => Self::from_plan(callable.returns().plan(), package),
            ErrorDecl::EncodedViaReturnSlot {
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_success_plan(callable.returns().plan(), package),
            ErrorDecl::EncodedViaReturnSlot { .. } => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible error buffer shape",
            }),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible callable stub",
            }),
        }
    }

    fn from_plan(plan: &ReturnPlan<Native, OutOfRust>, package: &Package<'_, '_>) -> Result<Self> {
        Ok(Self {
            annotation: PythonTypeHint::from_return(plan, package)?.into_string(),
            value: ReturnedValue::from_plan(plan, package)?,
        })
    }

    fn from_success_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        Ok(Self {
            annotation: PythonTypeHint::from_return(plan, package)?.into_string(),
            value: ReturnedValue::from_success_plan(plan, package)?,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallableBody {
    asynchronous: bool,
    lines: Vec<String>,
}

impl CallableBody {
    fn from_callable(
        callable: &ExportedCallable<Native>,
        native_name: &str,
        native_call: String,
        returned: &ReturnedValue,
    ) -> Result<Self> {
        match callable.execution() {
            ExecutionDecl::Synchronous(_) => Ok(Self::sync(returned.statement(native_call))),
            ExecutionDecl::Asynchronous(native::AsyncProtocol::PollHandle { .. }) => {
                Ok(Self::native_future(native_name, native_call, returned))
            }
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "unknown async callable",
            }),
        }
    }

    fn is_async(&self) -> bool {
        self.asynchronous
    }

    fn uses_async_helpers(&self) -> bool {
        self.asynchronous
    }

    fn into_lines(self) -> Vec<String> {
        self.lines
    }

    fn sync(line: String) -> Self {
        Self {
            asynchronous: false,
            lines: vec![line],
        }
    }

    fn native_future(native_name: &str, native_call: String, returned: &ReturnedValue) -> Self {
        let future = "__boltffi_future";
        Self {
            asynchronous: true,
            lines: std::iter::once(format!("{future} = _BoltFfiNativeFuture("))
                .chain(std::iter::once(format!("    {native_call},")))
                .chain(
                    ["poll", "complete", "cancel", "free", "panic_message"]
                        .into_iter()
                        .map(|suffix| format!("    _native.{native_name}__{suffix},")),
                )
                .chain(std::iter::once(")".to_owned()))
                .chain(returned.awaited_statement(format!("{future}.wait()")))
                .collect(),
        }
    }
}

enum ReturnedValue {
    Void,
    Native,
    ClassHandle(String),
    Wire(String),
}

impl ReturnedValue {
    fn from_plan(plan: &ReturnPlan<Native, OutOfRust>, package: &Package<'_, '_>) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self::Void),
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Ok(Self::ClassHandle(package.class_name(class_id)?)),
            ReturnPlan::EncodedViaReturnSlot {
                ty,
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_encoded_type(ty, package),
            _ => Ok(Self::Native),
        }
    }

    fn from_success_plan(
        plan: &ReturnPlan<Native, OutOfRust>,
        package: &Package<'_, '_>,
    ) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self::Void),
            ReturnPlan::HandleViaOutPointer {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Ok(Self::ClassHandle(package.class_name(class_id)?)),
            ReturnPlan::EncodedViaOutPointer {
                ty,
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_encoded_type(ty, package),
            ReturnPlan::DirectViaOutPointer { .. } => Ok(Self::Native),
            _ => Err(Error::UnsupportedTarget {
                target: "python",
                shape: "fallible success stub",
            }),
        }
    }

    fn statement(&self, native_call: String) -> String {
        match self {
            Self::Void => native_call,
            Self::Native | Self::ClassHandle(_) | Self::Wire(_) => {
                format!("return {}", self.expression(native_call))
            }
        }
    }

    fn expression(&self, native_call: String) -> String {
        match self {
            Self::Void => native_call,
            Self::Native => native_call,
            Self::ClassHandle(class_name) => {
                format!("{class_name}._from_handle({native_call})")
            }
            Self::Wire(decode) => {
                format!("_boltffi_read_wire({native_call}, lambda reader: {decode})")
            }
        }
    }

    fn uses_wire_helpers(&self) -> bool {
        matches!(self, Self::Wire(_))
    }

    fn awaited_statement(&self, wait_call: String) -> Vec<String> {
        let value = "__boltffi_value";
        match self {
            Self::Void => vec![format!("await {wait_call}")],
            Self::Native => vec![format!("return await {wait_call}")],
            Self::ClassHandle(class_name) => vec![
                format!("{value} = await {wait_call}"),
                format!("return {class_name}._from_handle({value})"),
            ],
            Self::Wire(decode) => vec![
                format!("{value} = await {wait_call}"),
                format!("return _boltffi_read_wire({value}, lambda reader: {decode})"),
            ],
        }
    }

    fn from_encoded_type(ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
        let encoded_type = match ty {
            TypeRef::Custom(custom_type) => package.custom_representation(*custom_type)?,
            _ => ty,
        };
        if matches!(
            encoded_type,
            TypeRef::Primitive(_)
                | TypeRef::String
                | TypeRef::Bytes
                | TypeRef::Record(_)
                | TypeRef::Enum(_)
        ) {
            Ok(Self::Native)
        } else {
            Ok(Self::Wire(
                WireValue::reader(encoded_type, package)?.decode(),
            ))
        }
    }
}

struct PythonTypeHint {
    annotation: String,
    uses_sequence: bool,
}

impl PythonTypeHint {
    fn from_type_ref(ty: &TypeRef, package: &Package<'_, '_>) -> Result<Self> {
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
            } => Ok(Self::new(package.record_name(*record)?)),
            ParamPlan::Direct {
                ty: TypeRef::Enum(enumeration),
                ..
            } => Ok(Self::new(package.enum_name(*enumeration)?)),
            ParamPlan::Handle {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Ok(Self::new(package.class_name(class_id)?)),
            ParamPlan::Handle {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Nullable,
                ..
            } => Ok(Self::new(format!(
                "{} | None",
                package.class_name(class_id)?
            ))),
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
            ParamPlan::Encoded {
                ty: TypeRef::Custom(custom_type),
                shape: native::BufferShape::Slice,
                ..
            } => {
                Self::from_parameter_type_ref(package.custom_representation(*custom_type)?, package)
            }
            ParamPlan::Encoded {
                ty: TypeRef::Record(record),
                shape: native::BufferShape::Slice,
                ..
            } => Ok(Self::new(package.record_name(*record)?)),
            ParamPlan::Encoded {
                ty: TypeRef::Enum(enumeration),
                shape: native::BufferShape::Slice,
                ..
            } => Ok(Self::new(package.enum_name(*enumeration)?)),
            ParamPlan::Encoded {
                ty,
                shape: native::BufferShape::Slice,
                ..
            } => Self::from_parameter_type_ref(ty, package),
            ParamPlan::DirectVec { element } => {
                Self::from_direct_vector_parameter(element, package)
            }
            ParamPlan::ScalarOption { primitive } => Ok(Self::new(format!(
                "{} | None",
                Self::from_primitive(*primitive)?.into_string()
            ))),
            ParamPlan::Direct { .. } | ParamPlan::Encoded { .. } | ParamPlan::Handle { .. } => {
                Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unsupported parameter stub",
                })
            }
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
            }
            | ReturnPlan::DirectViaOutPointer {
                ty: TypeRef::Primitive(primitive),
            } => Self::from_primitive(*primitive),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Record(record),
            }
            | ReturnPlan::DirectViaOutPointer {
                ty: TypeRef::Record(record),
            } => Ok(Self::new(package.record_name(*record)?)),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Enum(enumeration),
            }
            | ReturnPlan::DirectViaOutPointer {
                ty: TypeRef::Enum(enumeration),
            } => Ok(Self::new(package.enum_name(*enumeration)?)),
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            }
            | ReturnPlan::HandleViaOutPointer {
                target: HandleTarget::Class(class_id),
                presence: HandlePresence::Required,
                ..
            } => Ok(Self::new(package.class_name(class_id)?)),
            ReturnPlan::HandleViaReturnSlot {
                target: HandleTarget::Callback(_),
                ..
            } => Ok(Self::new("object")),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            }
            | ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::new("str")),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Buffer,
                ..
            }
            | ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::new("bytes")),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Custom(custom_type),
                shape: native::BufferShape::Buffer,
                ..
            }
            | ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::Custom(custom_type),
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_type_ref(package.custom_representation(*custom_type)?, package),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Record(record),
                shape: native::BufferShape::Buffer,
                ..
            }
            | ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::Record(record),
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::new(package.record_name(*record)?)),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Enum(enumeration),
                shape: native::BufferShape::Buffer,
                ..
            }
            | ReturnPlan::EncodedViaOutPointer {
                ty: TypeRef::Enum(enumeration),
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self::new(package.enum_name(*enumeration)?)),
            ReturnPlan::EncodedViaReturnSlot {
                ty,
                shape: native::BufferShape::Buffer,
                ..
            }
            | ReturnPlan::EncodedViaOutPointer {
                ty,
                shape: native::BufferShape::Buffer,
                ..
            } => Self::from_type_ref(ty, package),
            ReturnPlan::DirectVecViaReturnSlot { element } => Ok(Self::new(format!(
                "list[{}]",
                Self::from_type_ref(element, package)?.into_string()
            ))),
            ReturnPlan::ScalarOptionViaReturnSlot { primitive } => Ok(Self::new(format!(
                "{} | None",
                Self::from_primitive(*primitive)?.into_string()
            ))),
            ReturnPlan::DirectViaReturnSlot { .. }
            | ReturnPlan::EncodedViaReturnSlot { .. }
            | ReturnPlan::HandleViaReturnSlot { .. }
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

    fn into_string(self) -> String {
        self.annotation
    }

    fn uses_sequence(&self) -> bool {
        self.uses_sequence
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

    fn from_builtin(builtin: BuiltinType) -> Self {
        match builtin {
            BuiltinType::Duration | BuiltinType::SystemTime => Self::new("float"),
            BuiltinType::Uuid | BuiltinType::Url => Self::new("str"),
        }
    }
}

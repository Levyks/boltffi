use std::path::PathBuf;

use askama::Template as AskamaTemplate;
use boltffi_binding::{
    Bindings, DeclarationRef, ErrorDecl, FunctionDecl, IncomingParam, IntoRust, Native, OutOfRust,
    ParamDecl, ParamPlan, Primitive, ReturnPlan, TypeRef, native,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{Error, FilePath, GeneratedFile, GeneratedOutput, Result},
    target::python::name_style::Name,
};

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.py", escape = "none")]
struct InitTemplate {
    module_name_literal: String,
    package_name_literal: String,
    package_version_literal: String,
    library_name: String,
    functions: Vec<String>,
}

#[derive(AskamaTemplate)]
#[template(path = "target/python/package.pyi", escape = "none")]
struct StubTemplate {
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
        let functions = self.functions();
        let stubs = functions
            .iter()
            .map(|function| FunctionStub::from_declaration(function))
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
                        functions: names,
                    }
                    .render()?,
                )?,
                self.file(
                    PathBuf::from(&module).join("__init__.pyi"),
                    StubTemplate { functions: stubs }.render()?,
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
    fn from_declaration(function: &FunctionDecl<Native>) -> Result<Self> {
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
                .map(ParameterStub::from_declaration)
                .collect::<Result<Vec<_>>>()?,
            return_annotation: PythonTypeHint::from_return(function.callable().returns().plan())?
                .into_string(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParameterStub {
    name: String,
    annotation: String,
}

impl ParameterStub {
    fn from_declaration(parameter: &ParamDecl<Native, IntoRust>) -> Result<Self> {
        let IncomingParam::Value(plan) = parameter.payload() else {
            return Err(Error::UnsupportedTarget {
                target: "python",
                shape: "closure parameter stub",
            });
        };
        Ok(Self {
            name: Name::new(parameter.name()).function(),
            annotation: PythonTypeHint::from_parameter(plan)?.into_string(),
        })
    }
}

struct PythonTypeHint {
    annotation: &'static str,
}

impl PythonTypeHint {
    fn from_parameter(plan: &ParamPlan<Native, IntoRust>) -> Result<Self> {
        match plan {
            ParamPlan::Direct {
                ty: TypeRef::Primitive(primitive),
                ..
            } => Self::from_primitive(*primitive),
            ParamPlan::Encoded {
                ty: TypeRef::String,
                shape: native::BufferShape::Slice,
                ..
            } => Ok(Self { annotation: "str" }),
            ParamPlan::Encoded {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Slice,
                ..
            } => Ok(Self {
                annotation: "bytes",
            }),
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

    fn from_return(plan: &ReturnPlan<Native, OutOfRust>) -> Result<Self> {
        match plan {
            ReturnPlan::Void => Ok(Self { annotation: "None" }),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => Self::from_primitive(*primitive),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::String,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self { annotation: "str" }),
            ReturnPlan::EncodedViaReturnSlot {
                ty: TypeRef::Bytes,
                shape: native::BufferShape::Buffer,
                ..
            } => Ok(Self {
                annotation: "bytes",
            }),
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
        self.annotation.to_owned()
    }

    fn from_primitive(primitive: Primitive) -> Result<Self> {
        Ok(match primitive {
            Primitive::Bool => Self { annotation: "bool" },
            Primitive::F32 | Primitive::F64 => Self {
                annotation: "float",
            },
            Primitive::I8
            | Primitive::U8
            | Primitive::I16
            | Primitive::U16
            | Primitive::I32
            | Primitive::U32
            | Primitive::I64
            | Primitive::U64
            | Primitive::ISize
            | Primitive::USize => Self { annotation: "int" },
            _ => {
                return Err(Error::UnsupportedTarget {
                    target: "python",
                    shape: "unsupported primitive type hint",
                });
            }
        })
    }
}

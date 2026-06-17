mod render;

use boltffi_binding::{
    Bindings, CallbackDecl, ClassDecl, ConstantDecl, CustomTypeDecl, EnumDecl, FunctionDecl,
    Native, RecordDecl, StreamDecl,
};

use crate::{
    bridge::python_cext::PythonCExtBridgeContract,
    core::{
        BindingCapability, BridgeCapability, CapabilityRequirements, Emitted, Error,
        GeneratedOutput, HostCapabilities, RenderContext, RenderedDeclaration, Result,
        contract::sealed, host,
    },
};

/// Python host renderer for a CPython extension bridge.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct PythonCExtHost;

impl PythonCExtHost {
    /// Creates a Python host renderer for a CPython extension module.
    pub const fn new() -> Self {
        Self
    }
}

impl host::HostBackend for PythonCExtHost {
    type Surface = Native;
    type Bridge = PythonCExtBridgeContract;

    fn name(&self) -> &'static str {
        "python"
    }

    fn binding_capabilities(&self) -> HostCapabilities {
        HostCapabilities::new()
            .unsupported(
                BindingCapability::Records,
                "Python records are not migrated yet",
            )
            .unsupported(
                BindingCapability::Enums,
                "Python enums are not migrated yet",
            )
            .stable(BindingCapability::Functions)
            .unsupported(
                BindingCapability::Classes,
                "Python classes are not migrated yet",
            )
            .unsupported(
                BindingCapability::Callbacks,
                "Python callbacks are not migrated yet",
            )
            .unsupported(
                BindingCapability::Streams,
                "Python streams are not migrated yet",
            )
            .unsupported(
                BindingCapability::Constants,
                "Python constants are not migrated yet",
            )
            .unsupported(
                BindingCapability::CustomTypes,
                "Python custom types are not migrated yet",
            )
    }

    fn bridge_capabilities(&self) -> CapabilityRequirements<BridgeCapability> {
        CapabilityRequirements::new().require(BridgeCapability::PythonExtension)
    }

    fn record(
        &self,
        _decl: &RecordDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "record",
        })
    }

    fn enumeration(
        &self,
        _decl: &EnumDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "enum",
        })
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Wrapper::from_declaration(decl, bridge)?.render()
    }

    fn class(
        &self,
        _decl: &ClassDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "class",
        })
    }

    fn callback(
        &self,
        _decl: &CallbackDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "callback",
        })
    }

    fn stream(
        &self,
        _decl: &StreamDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "stream",
        })
    }

    fn constant(
        &self,
        _decl: &ConstantDecl<Self::Surface>,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "constant",
        })
    }

    fn custom_type(
        &self,
        _decl: &CustomTypeDecl,
        _bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        Err(Error::UnsupportedTarget {
            target: self.name(),
            shape: "custom type",
        })
    }

    fn assemble(
        &self,
        bindings: &Bindings<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'_, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        Ok(GeneratedOutput::combine([
            render::NativeModule::new(bridge, declarations).render()?,
            render::Package::new(bindings, bridge).render()?,
        ]))
    }
}

impl sealed::HostBackend for PythonCExtHost {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Bindings, Native, lower};

    use crate::{
        bridge::{c::CBridge, python_cext::PythonCExtBridge},
        core::{BridgeLayer, Error, GeneratedOutput, Target},
        target::python::PythonCExtHost,
    };

    fn empty_bindings() -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str("").expect("valid empty source"),
            PackageInfo::new("demo", None),
        )
        .expect("empty source should scan");
        lower::<Native>(&source).expect("empty source should lower")
    }

    fn bindings(source: &str) -> Bindings<Native> {
        let source = boltffi_scan::scan_file(
            syn::parse_str(source).expect("valid source"),
            PackageInfo::new("demo", None),
        )
        .expect("source should scan");
        lower::<Native>(&source).expect("source should lower")
    }

    fn target() -> Target<PythonCExtHost, BridgeLayer<CBridge, PythonCExtBridge>> {
        Target::new(
            PythonCExtHost::new(),
            BridgeLayer::new(
                CBridge::default_header().expect("C header bridge"),
                PythonCExtBridge::native_module().expect("CPython extension bridge"),
            ),
        )
    }

    fn extension(output: &GeneratedOutput) -> &str {
        file(output, "_native.c")
    }

    fn file<'output>(output: &'output GeneratedOutput, path: &str) -> &'output str {
        output
            .files()
            .iter()
            .find(|file| file.path().as_path() == Path::new(path))
            .map(|file| file.contents())
            .expect("generated file")
    }

    #[test]
    fn python_target_completes_cpython_module() {
        let output = target()
            .render(&empty_bindings())
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("static PyObject *boltffi_python_initialize_loader"));
        assert!(extension.contains("static PyMethodDef boltffi_python_methods[]"));
        assert!(extension.contains("{\"_initialize_loader\","));
        assert!(extension.contains("static struct PyModuleDef boltffi_python_module"));
        assert!(extension.contains("PyMODINIT_FUNC PyInit__native(void)"));
    }

    #[test]
    fn python_target_renders_primitive_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("static int boltffi_python_parse_i32"));
        assert!(extension.contains("static PyObject *boltffi_python_box_i32"));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_function_demo_add"
        ));
        assert!(extension.contains("if (nargs != 2)"));
        assert!(extension.contains("boltffi_python_parse_i32(args[0], &left)"));
        assert!(extension.contains("boltffi_python_parse_i32(args[1], &right)"));
        assert!(extension.contains(
            "result = boltffi_python_box_i32(boltffi_python_boltffi_function_demo_add(left, right));"
        ));
        assert!(extension.contains("return result;"));
        assert!(extension.contains(
            "{\"add\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_function_demo_add, METH_FASTCALL, NULL}"
        ));
    }

    #[test]
    fn python_target_emits_package_files_for_free_functions() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn add(left: i32, right: i32) -> i32 {
                    left + right
                }
                "#,
            ))
            .expect("Python target should render");
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");
        let setup = file(&output, "setup.py");

        assert!(file(&output, "pyproject.toml").contains("setuptools>=68"));
        assert_eq!(file(&output, "demo/py.typed"), "");
        assert!(init.contains("from . import _native"));
        assert!(init.contains("_native._initialize_loader"));
        assert!(init.contains("add = _native.add"));
        assert!(init.contains("MODULE_NAME = \"demo\""));
        assert!(init.contains("PACKAGE_NAME = \"demo\""));
        assert!(init.contains("\"add\","));
        assert!(stub.contains("def add(left: int, right: int) -> int: ..."));
        assert!(setup.contains("Extension(\n            \"demo._native\","));
        assert!(setup.contains("sources=[\"_native.c\"]"));
    }

    #[test]
    fn python_target_escapes_c_parameter_identifiers() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(r#int: i32) -> i32 {
                    r#int
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("int32_t int_;"));
        assert!(extension.contains("boltffi_python_parse_i32(args[0], &int_)"));
        assert!(extension.contains("boltffi_python_boltffi_function_demo_echo(int_)"));
    }

    #[test]
    fn python_target_escapes_python_method_keywords() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn lambda() -> i32 {
                    1
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains(
            "{\"lambda_\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_function_demo_lambda, METH_FASTCALL, NULL}"
        ));
    }

    #[test]
    fn python_target_rejects_async_functions() {
        let error = target()
            .render(&bindings(
                r#"
                #[export]
                pub async fn fetch() -> i32 {
                    1
                }
                "#,
            ))
            .expect_err("async Python functions must reject");

        assert_eq!(
            error,
            Error::UnsupportedTarget {
                target: "python",
                shape: "async function"
            }
        );
    }

    #[test]
    fn python_target_requires_bool_objects_for_bool_parameters() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(flag: bool) -> bool {
                    flag
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("if (!PyBool_Check(value))"));
        assert!(extension.contains("PyErr_SetString(PyExc_TypeError, \"expected bool\")"));
        assert!(extension.contains("*out = value == Py_True;"));
    }

    #[test]
    fn python_target_renders_string_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn greet(name: String) -> String {
                    name
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("static int boltffi_python_wire_string"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_utf8"));
        assert!(extension.contains("PyObject *name_wire = NULL;"));
        assert!(extension.contains("const uint8_t *name_ptr = NULL;"));
        assert!(extension.contains("uintptr_t name_len = 0;"));
        assert!(
            extension
                .contains("boltffi_python_wire_string(args[0], &name_wire, &name_ptr, &name_len)")
        );
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_utf8(boltffi_python_boltffi_function_demo_greet(name_ptr, name_len));"
        ));
        assert!(extension.contains("Py_XDECREF(name_wire);"));
        assert!(extension.contains("boltffi_python_boltffi_free_buf(buffer);"));
    }

    #[test]
    fn python_target_renders_bytes_function_wrapper() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub fn echo(bytes: Vec<u8>) -> Vec<u8> {
                    bytes
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);

        assert!(extension.contains("static int boltffi_python_wire_bytes"));
        assert!(extension.contains("static PyObject *boltffi_python_decode_owned_bytes"));
        assert!(extension.contains("PyObject *bytes_wire = NULL;"));
        assert!(extension.contains("const uint8_t *bytes_ptr = NULL;"));
        assert!(extension.contains("uintptr_t bytes_len = 0;"));
        assert!(
            extension.contains(
                "boltffi_python_wire_bytes(args[0], &bytes_wire, &bytes_ptr, &bytes_len)"
            )
        );
        assert!(extension.contains(
            "result = boltffi_python_decode_owned_bytes(boltffi_python_boltffi_function_demo_echo(bytes_ptr, bytes_len));"
        ));
        assert!(extension.contains("Py_XDECREF(bytes_wire);"));
    }
}

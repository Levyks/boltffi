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
            .stable(BindingCapability::Records)
            .stable(BindingCapability::Enums)
            .stable(BindingCapability::Functions)
            .stable(BindingCapability::Classes)
            .stable(BindingCapability::Callbacks)
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
        decl: &RecordDecl<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::RecordWrapper::from_declaration(decl, bridge)?.render()
    }

    fn enumeration(
        &self,
        decl: &EnumDecl<Self::Surface>,
        bridge: &Self::Bridge,
        _context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::EnumWrapper::from_declaration(decl, bridge)?.render()
    }

    fn function(
        &self,
        decl: &FunctionDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::Wrapper::from_declaration(decl, bridge, context)?.render()
    }

    fn class(
        &self,
        decl: &ClassDecl<Self::Surface>,
        bridge: &Self::Bridge,
        context: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::ClassWrapper::from_declaration(decl, bridge, context)?.render()
    }

    fn callback(
        &self,
        decl: &CallbackDecl<Self::Surface>,
        bridge: &Self::Bridge,
        _: &RenderContext<Self::Surface>,
    ) -> Result<Emitted> {
        render::CallbackWrapper::from_declaration(decl, bridge)?.render()
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
        context: &RenderContext<Self::Surface>,
        declarations: Vec<RenderedDeclaration<'_, Self::Surface>>,
    ) -> Result<GeneratedOutput> {
        Ok(GeneratedOutput::combine([
            render::NativeModule::new(bridge, context, declarations).render()?,
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
    fn python_target_renders_direct_record_package_and_native_conversions() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(C)]
                #[data]
                pub struct Point {
                    pub x: f64,
                    pub y: f64,
                }

                #[export]
                pub fn echo_point(value: Point) -> Point {
                    value
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("static PyObject *boltffi_python_point_type = NULL;"));
        assert!(extension.contains("static PyObject *boltffi_python_wrapper_register_point"));
        assert!(extension.contains("static int boltffi_python_parse_point"));
        assert!(extension.contains("static PyObject *boltffi_python_box_point"));
        assert!(extension.contains("___Point value;"));
        assert!(extension.contains("boltffi_python_parse_point(args[0], &value)"));
        assert!(extension.contains(
            "result = boltffi_python_box_point(boltffi_python_boltffi_function_demo_echo_point(value));"
        ));
        assert!(extension.contains(
            "{\"_register_point\", (PyCFunction)boltffi_python_wrapper_register_point, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains("Py_CLEAR(boltffi_python_point_type);"));
        assert!(init.contains("@dataclass(frozen=True, slots=True)\nclass Point:"));
        assert!(init.contains("    x: float"));
        assert!(init.contains("    y: float"));
        assert!(init.contains("_native._register_point(Point)"));
        assert!(stub.contains("class Point:"));
        assert!(stub.contains("def echo_point(value: Point) -> Point: ..."));
    }

    #[test]
    fn python_target_renders_c_style_enum_package_and_native_conversions() {
        let output = target()
            .render(&bindings(
                r#"
                #[repr(i32)]
                #[data]
                pub enum Status {
                    Active = 0,
                    Inactive = 1,
                    Pending = 2,
                }

                #[export]
                pub fn echo_status(value: Status) -> Status {
                    value
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains("typedef struct boltffi_python_c_style_enum_registration"));
        assert!(extension.contains(
            "static boltffi_python_c_style_enum_registration boltffi_python_status_registration"
        ));
        assert!(extension.contains("static PyObject *boltffi_python_wrapper_register_status"));
        assert!(extension.contains("static int boltffi_python_parse_status"));
        assert!(extension.contains("static PyObject *boltffi_python_box_status"));
        assert!(extension.contains("___Status value;"));
        assert!(extension.contains("boltffi_python_parse_status(args[0], &value)"));
        assert!(extension.contains(
            "result = boltffi_python_box_status(boltffi_python_boltffi_function_demo_echo_status(value));"
        ));
        assert!(extension.contains(
            "{\"_register_status\", (PyCFunction)boltffi_python_wrapper_register_status, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains(
            "boltffi_python_clear_c_style_enum_registration(&boltffi_python_status_registration);"
        ));
        assert!(init.contains("from enum import IntEnum"));
        assert!(init.contains("class Status(IntEnum):"));
        assert!(init.contains("    ACTIVE = 0"));
        assert!(init.contains("    INACTIVE = 1"));
        assert!(init.contains("    PENDING = 2"));
        assert!(init.contains("_native._register_status(Status)"));
        assert!(stub.contains("class Status(IntEnum):"));
        assert!(stub.contains("def echo_status(value: Status) -> Status: ..."));
    }

    #[test]
    fn python_target_renders_class_package_and_native_handle_wrappers() {
        let output = target()
            .render(&bindings(
                r#"
                pub struct Marker {
                    value: u32,
                }

                #[export(single_threaded)]
                impl Marker {
                    pub fn value(&self) -> u32 {
                        self.value
                    }
                }

                pub struct Engine {
                    value: u32,
                }

                #[export(single_threaded)]
                impl Engine {
                    pub fn new(value: u32) -> Self {
                        Self { value }
                    }

                    pub fn value(&self) -> u32 {
                        self.value
                    }

                    pub fn reset(&mut self) {
                        self.value = 0;
                    }

                    pub fn marker(&self) -> Marker {
                        Marker { value: self.value }
                    }

                    pub fn make_marker(value: u32) -> Marker {
                        Marker { value }
                    }
                }
                "#,
            ))
            .expect("Python target should render");
        let extension = extension(&output);
        let init = file(&output, "demo/__init__.py");
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_release_class_demo_engine"
        ));
        assert!(extension.contains(
            "static PyObject *boltffi_python_callable_wrapper_boltffi_init_class_demo_engine_new"
        ));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_value"));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_reset"));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_marker"));
        assert!(extension.contains("static PyObject *boltffi_python_callable_wrapper_boltffi_method_class_demo_engine_make_marker"));
        assert!(extension.contains("boltffi_python_parse_u64(args[0], &receiver)"));
        assert!(
            extension.contains("boltffi_python_boltffi_method_class_demo_engine_value(receiver)")
        );
        assert!(
            extension.contains("boltffi_python_boltffi_method_class_demo_engine_reset(receiver)")
        );
        assert!(extension.contains(
            "{\"_boltffi_engine_release\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_release_class_demo_engine, METH_FASTCALL, NULL}"
        ));
        assert!(extension.contains(
            "{\"_boltffi_engine_new\", (PyCFunction)boltffi_python_callable_wrapper_boltffi_init_class_demo_engine_new, METH_FASTCALL, NULL}"
        ));
        assert!(init.contains("class Engine:"));
        assert!(init.contains("__slots__ = (\"_handle\",)"));
        assert!(init.contains("def __init__(self, value: int) -> None:"));
        assert!(init.contains("self._handle = _native._boltffi_engine_new(value)"));
        assert!(init.contains("def __del__(self) -> None:"));
        assert!(init.contains("_native._boltffi_engine_release(handle)"));
        assert!(init.contains("def value(self) -> int:"));
        assert!(init.contains("return _native._boltffi_engine_value(self._handle)"));
        assert!(init.contains("def reset(self) -> None:"));
        assert!(init.contains("_native._boltffi_engine_reset(self._handle)"));
        assert!(init.contains("def marker(self) -> Marker:"));
        assert!(
            init.contains(
                "return Marker._from_handle(_native._boltffi_engine_marker(self._handle))"
            )
        );
        assert!(init.contains("def make_marker(value: int) -> Marker:"));
        assert!(
            init.contains("return Marker._from_handle(_native._boltffi_engine_make_marker(value))")
        );
        assert!(stub.contains("class Engine:"));
        assert!(stub.contains("_handle: int"));
        assert!(stub.contains("def __init__(self, value: int) -> None: ..."));
        assert!(stub.contains("def value(self) -> int: ..."));
        assert!(stub.contains("def reset(self) -> None: ..."));
        assert!(stub.contains("def marker(self) -> Marker: ..."));
        assert!(stub.contains("def make_marker(value: int) -> Marker: ..."));
    }

    #[test]
    fn python_target_renders_primitive_callback_handles() {
        let output = target()
            .render(&bindings(
                r#"
                #[export]
                pub trait ValueCallback {
                    fn on_value(&self, value: i32) -> i32;
                }

                #[export]
                pub fn invoke_value_callback(callback: impl ValueCallback, input: i32) -> i32 {
                    callback.on_value(input)
                }
                "#,
            ))
            .expect("Python target should render callback handles");
        let extension = extension(&output);
        let stub = file(&output, "demo/__init__.pyi");

        assert!(extension.contains(
            "static ___ValueCallbackVTable boltffi_python_callback_value_callback_vtable"
        ));
        assert!(extension.contains(".free = boltffi_python_callback_value_callback_free"));
        assert!(extension.contains(".clone = boltffi_python_callback_value_callback_clone"));
        assert!(extension.contains(".on_value = boltffi_python_callback_value_callback_on_value"));
        assert!(extension.contains("static int boltffi_python_bind_callback_value_callback"));
        assert!(extension.contains("boltffi_python_boltffi_register_callback_demo_value_callback(&boltffi_python_callback_value_callback_vtable)"));
        assert!(extension.contains("static int boltffi_python_parse_callback_value_callback"));
        assert!(extension.contains(
            "boltffi_python_boltffi_create_callback_demo_value_callback((uint64_t)(uintptr_t)value)"
        ));
        assert!(extension.contains("BoltFFICallbackHandle callback;"));
        assert!(
            extension.contains("boltffi_python_parse_callback_value_callback(args[0], &callback)")
        );
        assert!(extension.contains(
            "boltffi_python_boltffi_function_demo_invoke_value_callback(callback, input)"
        ));
        assert!(
            stub.contains("def invoke_value_callback(callback: object, input: int) -> int: ...")
        );
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

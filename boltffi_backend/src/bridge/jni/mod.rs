//! JNI bridge.
//!
//! This bridge layers above the C ABI bridge. It emits C functions with
//! JNI-exported names and gives JVM hosts a typed native-method contract.

mod bridge;
mod contract;
mod name;
mod template;

pub use bridge::JniBridge;
pub use contract::{
    BytesParameter, CallbackArgument, CallbackBytesArgument, CallbackCParameter,
    CallbackClosureArgument, CallbackClosureHandle, CallbackCompletionArgument,
    CallbackHandleArgument, CallbackMethod, CallbackParameter, CallbackRecordArgument,
    CallbackRegistration, CallbackReturn, ClosureArgument, ClosureParameter, ClosureRegistration,
    ContinuationParameter, JniBridgeContract, JniType, JvmMethodReturn, NativeMethod,
    NativeParameter, NativeParameterKind, NativeReturn, RecordParameter, RecordValue,
    ScalarParameter, ScalarReturn,
};
pub use name::{JniSymbolName, JvmClassPath, JvmNameSegment};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Native, lower};

    use crate::{
        bridge::{
            c::CBridge,
            jni::{JniBridge, JniBridgeContract},
        },
        core::{BridgeLayer, BridgeOutput, BridgeStack},
    };

    fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
        let file = syn::parse_str(source).expect("valid source fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("fixture should scan");
        lower::<Native>(&source).expect("fixture should lower")
    }

    fn bridge(source: &str) -> BridgeOutput<JniBridgeContract> {
        let bindings = bindings(source);
        let stack = BridgeLayer::new(
            CBridge::new("jni/demo.h").expect("C header bridge"),
            JniBridge::new("com.boltffi.demo", "Native", "jni/jni_glue.c").expect("JNI bridge"),
        );
        stack.build(&bindings).expect("JNI bridge stack")
    }

    fn files(source: &str) -> Vec<(String, String)> {
        bridge(source)
            .output()
            .files()
            .iter()
            .map(|file| {
                (
                    file.path().as_path().display().to_string(),
                    file.contents().to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn jni_bridge_layers_primitive_functions_on_c_bridge() {
        let files = files(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }

            #[export]
            pub fn enabled(flag: bool) -> bool {
                flag
            }

            #[export]
            pub fn refresh() {}

            #[export]
            pub fn echo_bytes(bytes: Vec<u8>) -> Vec<u8> {
                bytes
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("int32_t boltffi_function_demo_add(int32_t left, int32_t right);"));
        assert!(source.contains("#include \"demo.h\""));
        assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1add(JNIEnv *env, jclass cls, jint left, jint right)"));
        assert!(source.contains("int32_t result = boltffi_function_demo_add(left, right);"));
        assert!(source.contains("return (jint)result;"));
        assert!(source.contains("JNIEXPORT jboolean JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1enabled(JNIEnv *env, jclass cls, jboolean flag)"));
        assert!(source.contains("return (jboolean)result;"));
        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1refresh(JNIEnv *env, jclass cls)"));
        assert!(source.contains("FfiStatus status = boltffi_function_demo_refresh();"));
        assert!(source.contains("boltffi_jni_throw_status(env, status);"));
        assert!(source.contains("JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1echo_1bytes(JNIEnv *env, jclass cls, jbyteArray bytes)"));
        assert!(source.contains("jbyte *__boltffi_bytes_ptr = NULL;"));
        assert!(source.contains("FfiBuf_u8 result = boltffi_function_demo_echo_bytes((const uint8_t *)__boltffi_bytes_ptr, (uintptr_t)__boltffi_bytes_len);"));
        assert!(source.contains("return boltffi_jni_buffer_to_byte_array(env, result);"));
    }

    #[test]
    fn jni_bridge_contract_records_class_and_source_path() {
        let output = bridge(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        );
        let contract = output.contract();

        assert_eq!(contract.class().as_java_path(), "com.boltffi.demo.Native");
        assert_eq!(
            contract.source_path().as_path(),
            Path::new("jni/jni_glue.c")
        );
        assert_eq!(contract.c_header().as_str(), "demo.h");
        assert_eq!(contract.methods().len(), 1);
        assert_eq!(
            contract.methods()[0].symbol().to_string(),
            "Java_com_boltffi_demo_Native_boltffi_1function_1demo_1add"
        );
    }

    #[test]
    fn jni_bridge_renders_direct_records_and_c_style_enums() {
        let files = files(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            #[export]
            pub fn echo_point(point: Point) -> Point {
                point
            }

            #[export]
            pub fn echo_mode(mode: Mode) -> Mode {
                mode
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("___Point boltffi_function_demo_echo_point(___Point point);"));
        assert!(header.contains("___Mode boltffi_function_demo_echo_mode(___Mode mode);"));
        assert!(source.contains("static bool boltffi_jni_read_record"));
        assert!(source.contains("static jbyteArray boltffi_jni_record_to_byte_array"));
        assert!(source.contains("JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1echo_1point(JNIEnv *env, jclass cls, jbyteArray point)"));
        assert!(source.contains("___Point __boltffi_point_value;"));
        assert!(source.contains("boltffi_jni_read_record(env, point, (uintptr_t)sizeof(___Point), &__boltffi_point_value)"));
        assert!(source.contains(
            "___Point result = boltffi_function_demo_echo_point(__boltffi_point_value);"
        ));
        assert!(source.contains(
            "return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));"
        ));
        assert!(source.contains("JNIEXPORT jbyte JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1echo_1mode(JNIEnv *env, jclass cls, jbyte mode)"));
        assert!(
            source.contains("___Mode result = boltffi_function_demo_echo_mode((___Mode)mode);")
        );
        assert!(source.contains("return (jbyte)result;"));
    }

    #[test]
    fn jni_bridge_renders_class_handles_and_methods() {
        let files = files(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new(seed: u64) -> Self {
                    Self
                }

                pub fn version() -> u32 {
                    1
                }

                pub fn score(&self, point: Point) -> u32 {
                    point.x as u32
                }

                pub fn advance(&mut self, delta: u32) {}
            }
            "#,
        );
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1release_1class_1demo_1engine(JNIEnv *env, jclass cls, jlong handle)"));
        assert!(source.contains("boltffi_release_class_demo_engine(handle);"));
        assert!(source.contains("JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1init_1class_1demo_1engine_1new(JNIEnv *env, jclass cls, jlong seed)"));
        assert!(source.contains("uint64_t result = boltffi_init_class_demo_engine_new(seed);"));
        assert!(source.contains("return (jlong)result;"));
        assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1class_1demo_1engine_1version(JNIEnv *env, jclass cls)"));
        assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1class_1demo_1engine_1score(JNIEnv *env, jclass cls, jlong receiver, jbyteArray point)"));
        assert!(source.contains(
            "uint32_t result = boltffi_method_class_demo_engine_score(receiver, __boltffi_point_value);"
        ));
        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1class_1demo_1engine_1advance(JNIEnv *env, jclass cls, jlong receiver, jint delta)"));
        assert!(source.contains(
            "FfiStatus status = boltffi_method_class_demo_engine_advance(receiver, delta);"
        ));
    }

    #[test]
    fn jni_bridge_casts_async_handles_and_callbacks_to_c_abi_types() {
        let files = files(
            r#"
            #[export]
            pub async fn fetch_count() -> u32 {
                7
            }
            "#,
        );
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(source.contains("JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1fetch_1count(JNIEnv *env, jclass cls)"));
        assert!(source.contains("RustFutureHandle result = boltffi_function_demo_fetch_count();"));
        assert!(source.contains("return (jlong)result;"));
        assert!(source.contains("JNI_OnLoad(JavaVM *vm, void *reserved)"));
        assert!(source.contains("FindClass(env, \"com/boltffi/demo/Native\")"));
        assert!(source.contains("boltffiFutureContinuationCallback"));
        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1function_1demo_1fetch_1count_1poll(JNIEnv *env, jclass cls, jlong handle, jlong callback_data)"));
        assert!(source.contains("boltffi_async_function_demo_fetch_count_poll((RustFutureHandle)handle, callback_data, boltffi_jni_continuation_callback);"));
        assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1function_1demo_1fetch_1count_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)"));
        assert!(source.contains("uint32_t result = boltffi_async_function_demo_fetch_count_complete((RustFutureHandle)handle, (FfiStatus *)out_status);"));
        assert!(source.contains("return (jint)result;"));
    }

    #[test]
    fn jni_bridge_renders_closure_parameters_from_contract_group() {
        let files = files(
            r#"
            #[export]
            pub fn install(callback: impl Fn(u32) -> u32) {}
            "#,
        );
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1install(JNIEnv *env, jclass cls, jlong callback)"));
        assert!(source.contains("static uint32_t boltffi_jni____closure__u32_to_u32_call(void *user_data, uint32_t arg0)"));
        assert!(source.contains("FindClass(env, \"com/boltffi/demo/ClosureU32ToU32Callbacks\")"));
        assert!(source.contains(
            "GetStaticMethodID(env, g____closure__u32_to_u32_class, \"call\", \"(JI)I\")"
        ));
        assert!(source.contains(
            "GetStaticMethodID(env, g____closure__u32_to_u32_class, \"free\", \"(J)V\")"
        ));
        assert!(source.contains("FfiStatus status = boltffi_function_demo_install(boltffi_jni____closure__u32_to_u32_call, (void *)callback, boltffi_jni____closure__u32_to_u32_release);"));
    }

    #[test]
    fn jni_bridge_renders_callback_handle_parameters() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub fn install(listener: impl Listener) {}
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains(
            "BoltFFICallbackHandle boltffi_create_callback_demo_listener(uint64_t handle);"
        ));
        assert!(
            header.contains(
                "FfiStatus boltffi_function_demo_install(BoltFFICallbackHandle listener);"
            )
        );
        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1install(JNIEnv *env, jclass cls, jlong listener)"));
        assert!(source.contains("FfiStatus status = boltffi_function_demo_install(boltffi_create_callback_demo_listener((uint64_t)listener));"));
        assert!(source.contains("boltffi_jni_throw_status(env, status);"));
        assert!(source.contains("static jclass g____ListenerVTable_class = NULL;"));
        assert!(source.contains(
            "static uint32_t ___ListenerVTable_on_value(uint64_t handle, uint32_t value)"
        ));
        assert!(source.contains("FindClass(env, \"com/boltffi/demo/ListenerCallbacks\")"));
        assert!(source.contains(
            "GetStaticMethodID(env, g____ListenerVTable_class, \"on_value\", \"(JI)I\")"
        ));
        assert!(
            source
                .contains("boltffi_register_callback_demo_listener(&g____ListenerVTable_vtable);")
        );
    }

    #[test]
    fn jni_bridge_renders_callback_byte_slice_parameters() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                fn on_name(&self, name: String);
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("void (*on_name)(uint64_t, const uint8_t *, uintptr_t);"));
        assert!(source.contains(
            "static void ___ListenerVTable_on_name(uint64_t handle, const uint8_t *name_ptr, uintptr_t name_len)"
        ));
        assert!(source.contains(
            "jbyteArray name = boltffi_jni_bytes_to_byte_array(env, name_ptr, name_len);"
        ));
        assert!(source.contains(
            "(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_on_name_method, (jlong)handle, name);"
        ));
        assert!(source.contains("(*env)->DeleteLocalRef(env, name);"));
        assert!(source.contains(
            "GetStaticMethodID(env, g____ListenerVTable_class, \"on_name\", \"(J[B)V\")"
        ));
    }

    #[test]
    fn jni_bridge_renders_callback_handle_method_parameters() {
        let files = files(
            r#"
            #[export]
            pub trait Child {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub trait Listener {
                fn on_child(&self, child: Box<dyn Child>);
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("void (*on_child)(uint64_t, BoltFFICallbackHandle);"));
        assert!(source.contains(
            "static void ___ListenerVTable_on_child(uint64_t handle, BoltFFICallbackHandle child)"
        ));
        assert!(source.contains("jlong __boltffi_child_handle = 0;"));
        assert!(source.contains(
            "__boltffi_child_handle = boltffi_jni_callback_handle_new_owned(env, child);"
        ));
        assert!(source.contains(
            "(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_on_child_method, (jlong)handle, __boltffi_child_handle);"
        ));
        assert!(source.contains(
            "GetStaticMethodID(env, g____ListenerVTable_class, \"on_child\", \"(JJ)V\")"
        ));
    }

    #[test]
    fn jni_bridge_renders_callback_record_parameters() {
        let files = files(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[export]
            pub trait Listener {
                fn on_point(&self, point: Point);
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("void (*on_point)(uint64_t, ___Point);"));
        assert!(
            source.contains(
                "static void ___ListenerVTable_on_point(uint64_t handle, ___Point point)"
            )
        );
        assert!(source.contains(
            "jbyteArray __boltffi_point_array = boltffi_jni_record_to_byte_array(env, &point, (uintptr_t)sizeof(point));"
        ));
        assert!(source.contains(
            "(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_on_point_method, (jlong)handle, __boltffi_point_array);"
        ));
        assert!(source.contains("(*env)->DeleteLocalRef(env, __boltffi_point_array);"));
        assert!(source.contains(
            "GetStaticMethodID(env, g____ListenerVTable_class, \"on_point\", \"(J[B)V\")"
        ));
    }

    #[test]
    fn jni_bridge_renders_callback_closure_parameters() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                fn install(&self, callback: impl Fn(u32) -> u32);
            }
            "#,
        );
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(source.contains("typedef struct"));
        assert!(source.contains("uint32_t (*call)(void *, uint32_t);"));
        assert!(source.contains("static jlong boltffi_jni____closure__u32_to_u32_handle_new(JNIEnv *env, uint32_t (*call)(void *, uint32_t), void *context, void (*release)(void *))"));
        assert!(source.contains("jlong __boltffi_callback_handle = 0;"));
        assert!(source.contains("__boltffi_callback_handle = boltffi_jni____closure__u32_to_u32_handle_new(env, callback_call, (void *)callback_context, callback_release);"));
        assert!(source.contains("(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_install_method, (jlong)handle, __boltffi_callback_handle);"));
        assert!(source.contains("closure->call(closure->context, (uint32_t)arg0);"));
        assert!(source.contains("return (jint)result;"));
        assert!(
            source.contains(
                "GetStaticMethodID(env, g____ListenerVTable_class, \"install\", \"(JJ)V\")"
            )
        );
    }

    #[test]
    fn jni_bridge_renders_callback_encoded_returns() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                fn name(&self) -> String;
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("FfiBuf_u8 (*name)(uint64_t);"));
        assert!(source.contains("static FfiBuf_u8 ___ListenerVTable_name(uint64_t handle)"));
        assert!(source.contains(
            "jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, g____ListenerVTable_class, g____ListenerVTable_name_method, (jlong)handle);"
        ));
        assert!(source.contains(
            "FfiBuf_u8 result = boltffi_jni_byte_array_to_buffer(env, __boltffi_return_array);"
        ));
        assert!(
            source
                .contains("GetStaticMethodID(env, g____ListenerVTable_class, \"name\", \"(J)[B\")")
        );
    }

    #[test]
    fn jni_bridge_renders_callback_record_returns() {
        let files = files(
            r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[export]
            pub trait Listener {
                fn point(&self) -> Point;
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains("___Point (*point)(uint64_t);"));
        assert!(source.contains("static ___Point ___ListenerVTable_point(uint64_t handle)"));
        assert!(source.contains(
            "jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, g____ListenerVTable_class, g____ListenerVTable_point_method, (jlong)handle);"
        ));
        assert!(source.contains(
            "if (!boltffi_jni_read_record(env, __boltffi_return_array, (uintptr_t)sizeof(result), &result))"
        ));
        assert!(
            source.contains(
                "GetStaticMethodID(env, g____ListenerVTable_class, \"point\", \"(J)[B\")"
            )
        );
    }

    #[test]
    fn jni_bridge_renders_async_callback_completions() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                async fn load(&self, key: u32) -> String;
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(header.contains(
            "void (*load)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);"
        ));
        assert!(
            source.contains("static void ___ListenerVTable_load(uint64_t handle, uint32_t key,")
        );
        assert!(source.contains("void (*complete)(void *, FfiStatus, FfiBuf_u8)"));
        assert!(source.contains("void *complete_context"));
        assert!(source.contains("(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_load_method, (jlong)handle, (jint)key, (jlong)complete, (jlong)complete_context);"));
        assert!(
            source.contains("complete(complete_context, (FfiStatus){.code = 1}, (FfiBuf_u8){0});")
        );
        assert!(
            source.contains(
                "GetStaticMethodID(env, g____ListenerVTable_class, \"load\", \"(JIJJ)V\")"
            )
        );
    }

    #[test]
    fn jni_bridge_renders_callback_handle_returns() {
        let files = files(
            r#"
            #[export]
            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub fn make_listener() -> Box<dyn Listener> {
                todo!()
            }
            "#,
        );
        let header = files
            .iter()
            .find(|(path, _)| path == "jni/demo.h")
            .map(|(_, contents)| contents)
            .expect("C header file");
        let source = files
            .iter()
            .find(|(path, _)| path == "jni/jni_glue.c")
            .map(|(_, contents)| contents)
            .expect("JNI source file");

        assert!(
            header.contains("BoltFFICallbackHandle boltffi_function_demo_make_listener(void);")
        );
        assert!(source.contains("static jlong boltffi_jni_callback_handle_new_owned"));
        assert!(source.contains("JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1callback_1handle_1clone(JNIEnv *env, jclass cls, jlong handle)"));
        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1callback_1handle_1release(JNIEnv *env, jclass cls, jlong handle)"));
        assert!(source.contains("JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1make_1listener(JNIEnv *env, jclass cls)"));
        assert!(
            source
                .contains("BoltFFICallbackHandle result = boltffi_function_demo_make_listener();")
        );
        assert!(source.contains("return boltffi_jni_callback_handle_new_owned(env, result);"));
    }
}

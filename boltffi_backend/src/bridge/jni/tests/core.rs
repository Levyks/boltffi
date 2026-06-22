use std::path::Path;

use super::{bridge, files};

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
    assert!(source.contains(
        "boltffi_jni_read_record(env, point, (uintptr_t)sizeof(___Point), &__boltffi_point_value)"
    ));
    assert!(
        source
            .contains("___Point result = boltffi_function_demo_echo_point(__boltffi_point_value);")
    );
    assert!(source.contains(
        "return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));"
    ));
    assert!(source.contains("JNIEXPORT jbyte JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1echo_1mode(JNIEnv *env, jclass cls, jbyte mode)"));
    assert!(source.contains("___Mode result = boltffi_function_demo_echo_mode((___Mode)mode);"));
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
    assert!(
        source.contains(
            "FfiStatus status = boltffi_method_class_demo_engine_advance(receiver, delta);"
        )
    );
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
    assert!(source.contains(
        "static uint32_t boltffi_jni____closure__u32_to_u32_call(void *user_data, uint32_t arg0)"
    ));
    assert!(source.contains("FindClass(env, \"com/boltffi/demo/ClosureU32ToU32Callbacks\")"));
    assert!(
        source.contains(
            "GetStaticMethodID(env, g____closure__u32_to_u32_class, \"call\", \"(JI)I\")"
        )
    );
    assert!(
        source
            .contains("GetStaticMethodID(env, g____closure__u32_to_u32_class, \"free\", \"(J)V\")")
    );
    assert!(source.contains("FfiStatus status = boltffi_function_demo_install(boltffi_jni____closure__u32_to_u32_call, (void *)callback, boltffi_jni____closure__u32_to_u32_release);"));
}

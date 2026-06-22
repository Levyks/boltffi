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
fn jni_bridge_renders_encoded_functions_as_byte_arrays() {
    let files = files(
        r#"
            #[data]
            pub struct Person {
                pub name: String,
            }

            #[data]
            pub enum Shape {
                Label(String),
            }

            #[export]
            pub fn keep_person(person: Person) -> Person {
                person
            }

            #[export]
            pub fn keep_shape(shape: Shape) -> Shape {
                shape
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
    [
        "FfiBuf_u8 boltffi_function_demo_keep_person(const uint8_t *person_ptr, uintptr_t person_len);",
        "FfiBuf_u8 boltffi_function_demo_keep_shape(const uint8_t *shape_ptr, uintptr_t shape_len);",
    ]
    .into_iter()
    .for_each(|expected| assert!(header.contains(expected), "{expected}\n{header}"));

    [
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1keep_1person(JNIEnv *env, jclass cls, jbyteArray person)",
        "jbyte *__boltffi_person_ptr = NULL;",
        "jsize __boltffi_person_len = 0;",
        "FfiBuf_u8 result = boltffi_function_demo_keep_person((const uint8_t *)__boltffi_person_ptr, (uintptr_t)__boltffi_person_len);",
        "return boltffi_jni_buffer_to_byte_array(env, result);",
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1keep_1shape(JNIEnv *env, jclass cls, jbyteArray shape)",
        "FfiBuf_u8 result = boltffi_function_demo_keep_shape((const uint8_t *)__boltffi_shape_ptr, (uintptr_t)__boltffi_shape_len);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
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
fn jni_bridge_renders_async_class_methods() {
    let files = files(
        r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new() -> Self {
                    Self
                }

                pub async fn compute(&self, value: u32) -> u32 {
                    value
                }
            }
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");
    [
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1class_1demo_1engine_1compute(JNIEnv *env, jclass cls, jlong receiver, jint value)",
        "RustFutureHandle result = boltffi_method_class_demo_engine_compute(receiver, value);",
        "return (jlong)result;",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1class_1demo_1engine_1compute_1poll(JNIEnv *env, jclass cls, jlong handle, jlong callback_data)",
        "boltffi_async_method_class_demo_engine_compute_poll((RustFutureHandle)handle, callback_data, boltffi_jni_continuation_callback);",
        "JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1class_1demo_1engine_1compute_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)",
        "uint32_t result = boltffi_async_method_class_demo_engine_compute_complete((RustFutureHandle)handle, (FfiStatus *)out_status);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1class_1demo_1engine_1compute_1cancel(JNIEnv *env, jclass cls, jlong handle)",
        "boltffi_async_method_class_demo_engine_compute_cancel((RustFutureHandle)handle);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1class_1demo_1engine_1compute_1free(JNIEnv *env, jclass cls, jlong handle)",
        "boltffi_async_method_class_demo_engine_compute_free((RustFutureHandle)handle);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
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
fn jni_bridge_renders_async_complete_return_shapes() {
    let files = files(
        r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[export]
            pub async fn refresh() {}

            #[export]
            pub async fn load_name() -> String {
                "bolt".to_owned()
            }

            #[export]
            pub async fn load_point() -> Point {
                Point { x: 1, y: 2 }
            }
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    [
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1function_1demo_1refresh_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)",
        "boltffi_async_function_demo_refresh_complete((RustFutureHandle)handle, (FfiStatus *)out_status);",
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1function_1demo_1load_1name_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)",
        "FfiBuf_u8 result = boltffi_async_function_demo_load_name_complete((RustFutureHandle)handle, (FfiStatus *)out_status);",
        "return boltffi_jni_buffer_to_byte_array(env, result);",
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1function_1demo_1load_1point_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)",
        "___Point result = boltffi_async_function_demo_load_point_complete((RustFutureHandle)handle, (FfiStatus *)out_status);",
        "return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
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

#[test]
fn jni_bridge_renders_encoded_closure_parameters_from_contract_group() {
    let files = files(
        r#"
            #[export]
            pub fn install(callback: impl Fn(String) -> String) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(source.contains(
            "static FfiBuf_u8 boltffi_jni____closure__string_to_string_call(void *user_data, const uint8_t *arg0_ptr, uintptr_t arg0_len)"
        ));
    assert!(source.contains("jbyteArray arg0 = NULL;"));
    assert!(source.contains("arg0 = boltffi_jni_bytes_to_byte_array(env, arg0_ptr, arg0_len);"));
    assert!(source.contains(
            "jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, g____closure__string_to_string_class, g____closure__string_to_string_call_method, handle, arg0);"
        ));
    assert!(source.contains("(*env)->DeleteLocalRef(env, arg0);"));
    assert!(source.contains(
        "GetStaticMethodID(env, g____closure__string_to_string_class, \"call\", \"(J[B)[B\")"
    ));
    assert!(source.contains(
            "FfiStatus status = boltffi_function_demo_install(boltffi_jni____closure__string_to_string_call, (void *)callback, boltffi_jni____closure__string_to_string_release);"
        ));
}

#[test]
fn jni_bridge_renders_encoded_closure_return_shapes_as_byte_arrays() {
    let files = files(
        r#"
            #[export]
            pub fn install_vec(callback: impl Fn() -> Vec<u32>) {}

            #[export]
            pub fn install_option(callback: impl Fn() -> Option<i32>) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    [
        "static FfiBuf_u8 boltffi_jni____closure__to_vec_u32_call(void *user_data)",
        "GetStaticMethodID(env, g____closure__to_vec_u32_class, \"call\", \"(J)[B\")",
        "static FfiBuf_u8 boltffi_jni____closure__to_opt_i32_call(void *user_data)",
        "GetStaticMethodID(env, g____closure__to_opt_i32_class, \"call\", \"(J)[B\")",
        "FfiBuf_u8 result = boltffi_jni_byte_array_to_buffer(env, __boltffi_return_array);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
}

#[test]
fn jni_bridge_renders_direct_vector_closure_parameters_from_contract_group() {
    let files = files(
        r#"
            #[export]
            pub fn install(callback: impl Fn(Vec<u32>) -> u32) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(source.contains(
            "static uint32_t boltffi_jni____closure__vec_u32_to_u32_call(void *user_data, const uint32_t *arg0_ptr, uintptr_t arg0_len)"
        ));
    assert!(source.contains("jintArray arg0 = NULL;"));
    assert!(source.contains("arg0 = (*env)->NewIntArray(env, (jsize)arg0_len);"));
    assert!(source.contains(
        "(*env)->SetIntArrayRegion(env, arg0, 0, (jsize)arg0_len, (const jint *)arg0_ptr);"
    ));
    assert!(source.contains(
        "uint32_t result = (uint32_t)(*env)->CallStaticIntMethod(env, g____closure__vec_u32_to_u32_class, g____closure__vec_u32_to_u32_call_method, handle, arg0);"
    ));
    assert!(source.contains(
        "GetStaticMethodID(env, g____closure__vec_u32_to_u32_class, \"call\", \"(J[I)I\")"
    ));
    assert!(source.contains(
            "FfiStatus status = boltffi_function_demo_install(boltffi_jni____closure__vec_u32_to_u32_call, (void *)callback, boltffi_jni____closure__vec_u32_to_u32_release);"
        ));
}

#[test]
fn jni_bridge_renders_nested_closure_parameters_from_contract_group() {
    let files = files(
        r#"
            #[export]
            pub fn install(callback: impl Fn(Box<dyn Fn(u32) -> u32>) -> u32) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    [
        "uint32_t (*arg0_call)(void *, uint32_t)",
        "jlong __boltffi_arg0_handle = 0;",
        "__boltffi_arg0_handle = boltffi_jni____closure__u32_to_u32_handle_new(env, arg0_call, (void *)arg0_context, arg0_release);",
        "boltffi_jni____closure__u32_to_u32_handle_release(__boltffi_arg0_handle);",
        "GetStaticMethodID(env, g____closure__box_closure_to_u32_class, \"call\", \"(JJ)I\")",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
}

#[test]
fn jni_bridge_renders_nested_closure_parameters_for_callback_owned_closures() {
    let files = files(
        r#"
            #[export]
            pub trait Listener {
                fn install(&self, callback: impl Fn(Box<dyn Fn(u32) -> u32>) -> u32);
            }
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    [
        "static jlong boltffi_jni____closure__box_closure_to_u32_handle_new(JNIEnv *env, uint32_t (*call)(void *, uint32_t (*)(void *, uint32_t), void *, void (*)(void *)), void *context, void (*release)(void *))",
        "JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1callback_1closure_1_1_1_1closure_1_1box_1closure_1to_1u32_1call(JNIEnv *env, jclass cls, jlong value, jlong arg0)",
        "uint32_t result = closure->call(closure->context, boltffi_jni____closure__u32_to_u32_call, (void *)arg0, boltffi_jni____closure__u32_to_u32_release);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
}

#[test]
fn jni_bridge_renders_closure_callback_handle_returns() {
    let files = files(
        r#"
            #[export]
            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub fn install(callback: impl Fn() -> Box<dyn Listener>) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(source.contains(
        "GetStaticMethodID(env, g____closure__to_box_demo_listener_class, \"call\", \"(J)J\")"
    ));
    assert!(source.contains(
        "static BoltFFICallbackHandle boltffi_jni____closure__to_box_demo_listener_call(void *user_data)"
    ));
    assert!(source.contains("jlong __boltffi_return_handle = (*env)->CallStaticLongMethod(env, g____closure__to_box_demo_listener_class, g____closure__to_box_demo_listener_call_method, handle);"));
    assert!(source.contains(
        "BoltFFICallbackHandle result = boltffi_create_callback_demo_listener((uint64_t)__boltffi_return_handle);"
    ));
}

#[test]
fn jni_bridge_renders_closure_direct_record_returns() {
    let files = files(
        r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[export]
            pub fn install(callback: impl Fn() -> Point) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(
        source.contains(
            "static ___Point boltffi_jni____closure__to_demo_point_call(void *user_data)"
        )
    );
    assert!(source.contains(
        "GetStaticMethodID(env, g____closure__to_demo_point_class, \"call\", \"(J)[B\")"
    ));
    assert!(source.contains("jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, g____closure__to_demo_point_class, g____closure__to_demo_point_call_method, handle);"));
    assert!(source.contains("___Point result = {0};"));
    assert!(source.contains(
        "boltffi_jni_read_record(env, __boltffi_return_array, (uintptr_t)sizeof(result), &result)"
    ));
}

#[test]
fn jni_bridge_renders_closure_class_handle_returns() {
    let files = files(
        r#"
            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new() -> Self {
                    Self
                }
            }

            #[export]
            pub fn install(callback: impl Fn() -> Engine) {}
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(
        source.contains(
            "static uint64_t boltffi_jni____closure__to_demo_engine_call(void *user_data)"
        )
    );
    assert!(source.contains(
        "GetStaticMethodID(env, g____closure__to_demo_engine_class, \"call\", \"(J)J\")"
    ));
    assert!(source.contains("uint64_t result = (uint64_t)(*env)->CallStaticLongMethod(env, g____closure__to_demo_engine_class, g____closure__to_demo_engine_call_method, handle);"));
    assert!(source.contains("return result;"));
}

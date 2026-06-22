use super::files;

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

    assert!(
        header.contains(
            "BoltFFICallbackHandle boltffi_create_callback_demo_listener(uint64_t handle);"
        )
    );
    assert!(
        header.contains("FfiStatus boltffi_function_demo_install(BoltFFICallbackHandle listener);")
    );
    assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1install(JNIEnv *env, jclass cls, jlong listener)"));
    assert!(source.contains("FfiStatus status = boltffi_function_demo_install(boltffi_create_callback_demo_listener((uint64_t)listener));"));
    assert!(source.contains("boltffi_jni_throw_status(env, status);"));
    assert!(source.contains("static jclass g____ListenerVTable_class = NULL;"));
    assert!(
        source.contains(
            "static uint32_t ___ListenerVTable_on_value(uint64_t handle, uint32_t value)"
        )
    );
    assert!(source.contains("FindClass(env, \"com/boltffi/demo/ListenerCallbacks\")"));
    assert!(
        source
            .contains("GetStaticMethodID(env, g____ListenerVTable_class, \"on_value\", \"(JI)I\")")
    );
    assert!(
        source.contains("boltffi_register_callback_demo_listener(&g____ListenerVTable_vtable);")
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
    assert!(
        source.contains(
            "jbyteArray name = boltffi_jni_bytes_to_byte_array(env, name_ptr, name_len);"
        )
    );
    assert!(source.contains(
            "(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_on_name_method, (jlong)handle, name);"
        ));
    assert!(source.contains("(*env)->DeleteLocalRef(env, name);"));
    assert!(
        source
            .contains("GetStaticMethodID(env, g____ListenerVTable_class, \"on_name\", \"(J[B)V\")")
    );
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
    assert!(
        source.contains(
            "__boltffi_child_handle = boltffi_jni_callback_handle_new_owned(env, child);"
        )
    );
    assert!(source.contains(
            "(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_on_child_method, (jlong)handle, __boltffi_child_handle);"
        ));
    assert!(
        source
            .contains("GetStaticMethodID(env, g____ListenerVTable_class, \"on_child\", \"(JJ)V\")")
    );
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
        source.contains("static void ___ListenerVTable_on_point(uint64_t handle, ___Point point)")
    );
    assert!(source.contains(
            "jbyteArray __boltffi_point_array = boltffi_jni_record_to_byte_array(env, &point, (uintptr_t)sizeof(point));"
        ));
    assert!(source.contains(
            "(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_on_point_method, (jlong)handle, __boltffi_point_array);"
        ));
    assert!(source.contains("(*env)->DeleteLocalRef(env, __boltffi_point_array);"));
    assert!(
        source.contains(
            "GetStaticMethodID(env, g____ListenerVTable_class, \"on_point\", \"(J[B)V\")"
        )
    );
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
        source
            .contains("GetStaticMethodID(env, g____ListenerVTable_class, \"install\", \"(JJ)V\")")
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
        source.contains("GetStaticMethodID(env, g____ListenerVTable_class, \"name\", \"(J)[B\")")
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
        source.contains("GetStaticMethodID(env, g____ListenerVTable_class, \"point\", \"(J)[B\")")
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
    assert!(source.contains("static void ___ListenerVTable_load(uint64_t handle, uint32_t key,"));
    assert!(source.contains("void (*complete)(void *, FfiStatus, FfiBuf_u8)"));
    assert!(source.contains("void *complete_context"));
    assert!(source.contains("(*env)->CallStaticVoidMethod(env, g____ListenerVTable_class, g____ListenerVTable_load_method, (jlong)handle, (jint)key, (jlong)complete, (jlong)complete_context);"));
    assert!(source.contains("complete(complete_context, (FfiStatus){.code = 1}, (FfiBuf_u8){0});"));
    assert!(
        source.contains("GetStaticMethodID(env, g____ListenerVTable_class, \"load\", \"(JIJJ)V\")")
    );
}

#[test]
fn jni_bridge_renders_async_callback_completion_shapes() {
    let files = files(
        r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: i32,
                pub y: i32,
            }

            #[repr(i32)]
            #[data]
            pub enum LoadError {
                Bad = 1,
            }

            #[export]
            pub trait Listener {
                async fn value(&self, key: u32) -> u32;
                async fn point(&self, point: Point) -> Point;
                async fn values(&self, count: u32) -> Vec<u32>;
                async fn try_load(&self, key: u32) -> Result<String, LoadError>;
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
        "void (*value)(uint64_t, uint32_t, void (*)(void *, FfiStatus, uint32_t), void *);",
        "void (*point)(uint64_t, ___Point, void (*)(void *, FfiStatus, ___Point), void *);",
        "void (*values)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);",
        "void (*try_load)(uint64_t, uint32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);",
    ]
    .into_iter()
    .for_each(|expected| assert!(header.contains(expected), "{expected}"));

    [
        "static void ___ListenerVTable_value(uint64_t handle, uint32_t key, void (*complete)(void *, FfiStatus, uint32_t), void *complete_context)",
        "complete(complete_context, (FfiStatus){.code = 1}, (uint32_t){0});",
        "static void ___ListenerVTable_point(uint64_t handle, ___Point point, void (*complete)(void *, FfiStatus, ___Point), void *complete_context)",
        "complete(complete_context, (FfiStatus){.code = 1}, (___Point){0});",
        "jbyteArray __boltffi_point_array = boltffi_jni_record_to_byte_array(env, &point, (uintptr_t)sizeof(point));",
        "static void ___ListenerVTable_values(uint64_t handle, uint32_t count, void (*complete)(void *, FfiStatus, FfiBuf_u8), void *complete_context)",
        "complete(complete_context, (FfiStatus){.code = 1}, (FfiBuf_u8){0});",
        "static void ___ListenerVTable_try_load(uint64_t handle, uint32_t key, void (*complete)(void *, FfiStatus, FfiBuf_u8), void *complete_context)",
        "GetStaticMethodID(env, g____ListenerVTable_class, \"value\", \"(JIJJ)V\")",
        "GetStaticMethodID(env, g____ListenerVTable_class, \"point\", \"(J[BJJ)V\")",
        "GetStaticMethodID(env, g____ListenerVTable_class, \"values\", \"(JIJJ)V\")",
        "GetStaticMethodID(env, g____ListenerVTable_class, \"try_load\", \"(JIJJ)V\")",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
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

    assert!(header.contains("BoltFFICallbackHandle boltffi_function_demo_make_listener(void);"));
    assert!(source.contains("static jlong boltffi_jni_callback_handle_new_owned"));
    assert!(source.contains("JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1callback_1handle_1clone(JNIEnv *env, jclass cls, jlong handle)"));
    assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1callback_1handle_1release(JNIEnv *env, jclass cls, jlong handle)"));
    assert!(source.contains("JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1make_1listener(JNIEnv *env, jclass cls)"));
    assert!(
        source.contains("BoltFFICallbackHandle result = boltffi_function_demo_make_listener();")
    );
    assert!(source.contains("return boltffi_jni_callback_handle_new_owned(env, result);"));
}

#[test]
fn jni_bridge_renders_nullable_callback_handle_returns() {
    let files = files(
        r#"
            use std::sync::Arc;

            #[export]
            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub fn optional_boxed_listener() -> Option<Box<dyn Listener>> {
                todo!()
            }

            #[export]
            pub fn optional_shared_listener() -> Option<Arc<dyn Listener>> {
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

    [
        "BoltFFICallbackHandle boltffi_function_demo_optional_boxed_listener(void);",
        "BoltFFICallbackHandle boltffi_function_demo_optional_shared_listener(void);",
    ]
    .into_iter()
    .for_each(|expected| assert!(header.contains(expected), "{expected}"));

    [
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1optional_1boxed_1listener(JNIEnv *env, jclass cls)",
        "BoltFFICallbackHandle result = boltffi_function_demo_optional_boxed_listener();",
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1optional_1shared_1listener(JNIEnv *env, jclass cls)",
        "BoltFFICallbackHandle result = boltffi_function_demo_optional_shared_listener();",
        "return boltffi_jni_callback_handle_new_owned(env, result);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
}

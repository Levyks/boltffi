use super::files;

#[test]
fn jni_bridge_renders_record_associated_callables() {
    let files = files(
        r#"
        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: f64,
            pub y: f64,
        }

        #[data]
        pub struct Person {
            pub name: String,
        }

        #[data(impl)]
        impl Point {
            pub fn origin() -> Self {
                todo!()
            }

            pub fn distance(&self, other: Point) -> f64 {
                other.x - self.x
            }
        }

        #[data(impl)]
        impl Person {
            pub fn rename(&self, name: String) -> String {
                name
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
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1init_1record_1demo_1point_1origin(JNIEnv *env, jclass cls)",
        "___Point result = boltffi_init_record_demo_point_origin();",
        "return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));",
        "JNIEXPORT jdouble JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1record_1demo_1point_1distance(JNIEnv *env, jclass cls, jbyteArray receiver, jbyteArray other)",
        "___Point __boltffi_receiver_value;",
        "___Point __boltffi_other_value;",
        "double result = boltffi_method_record_demo_point_distance(__boltffi_receiver_value, __boltffi_other_value);",
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1record_1demo_1person_1rename(JNIEnv *env, jclass cls, jbyteArray receiver, jbyteArray name)",
        "FfiBuf_u8 result = boltffi_method_record_demo_person_rename((const uint8_t *)__boltffi_receiver_ptr, (uintptr_t)__boltffi_receiver_len, (const uint8_t *)__boltffi_name_ptr, (uintptr_t)__boltffi_name_len);",
        "return boltffi_jni_buffer_to_byte_array(env, result);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
}

#[test]
fn jni_bridge_renders_async_record_methods() {
    let files = files(
        r#"
        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: f64,
            pub y: f64,
        }

        #[data(impl)]
        impl Point {
            pub async fn compute(&self) -> f64 {
                self.x
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
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1record_1demo_1point_1compute(JNIEnv *env, jclass cls, jbyteArray receiver)",
        "RustFutureHandle result = boltffi_method_record_demo_point_compute(__boltffi_receiver_value);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1record_1demo_1point_1compute_1poll(JNIEnv *env, jclass cls, jlong handle, jlong callback_data)",
        "boltffi_async_method_record_demo_point_compute_poll((RustFutureHandle)handle, callback_data, boltffi_jni_continuation_callback);",
        "JNIEXPORT jdouble JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1record_1demo_1point_1compute_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)",
        "double result = boltffi_async_method_record_demo_point_compute_complete((RustFutureHandle)handle, (FfiStatus *)out_status);",
        "return (jdouble)result;",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1record_1demo_1point_1compute_1cancel(JNIEnv *env, jclass cls, jlong handle)",
        "boltffi_async_method_record_demo_point_compute_cancel((RustFutureHandle)handle);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1record_1demo_1point_1compute_1free(JNIEnv *env, jclass cls, jlong handle)",
        "boltffi_async_method_record_demo_point_compute_free((RustFutureHandle)handle);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
}

#[test]
fn jni_bridge_writes_mutable_direct_record_receivers_back() {
    let files = files(
        r#"
        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: f64,
            pub y: f64,
        }

        #[data(impl)]
        impl Point {
            pub fn move_by(&mut self, dx: f64) {
                self.x += dx;
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
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1record_1demo_1point_1move_1by(JNIEnv *env, jclass cls, jbyteArray receiver, jdouble dx)",
        "___Point __boltffi_receiver_value;",
        "___Point __boltffi_receiver_out;",
        "FfiStatus status = boltffi_method_record_demo_point_move_by(__boltffi_receiver_value, &__boltffi_receiver_out, dx);",
        "if (status.code == 0) {",
        "(*env)->SetByteArrayRegion(env, receiver, 0, (jsize)sizeof(___Point), (const jbyte *)&__boltffi_receiver_out);",
        "boltffi_jni_throw_status(env, status);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
}

#[test]
fn jni_bridge_renders_enum_associated_callables() {
    let files = files(
        r#"
        #[repr(u8)]
        #[data]
        pub enum Mode {
            Fast = 1,
            Slow = 2,
        }

        #[data(impl)]
        impl Mode {
            pub fn default() -> Self {
                todo!()
            }

            pub fn code(&self) -> u8 {
                0
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
        "JNIEXPORT jbyte JNICALL Java_com_boltffi_demo_Native_boltffi_1init_1enum_1demo_1mode_1default(JNIEnv *env, jclass cls)",
        "___Mode result = boltffi_init_enum_demo_mode_default();",
        "return (jbyte)result;",
        "JNIEXPORT jbyte JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1enum_1demo_1mode_1code(JNIEnv *env, jclass cls, jbyte receiver)",
        "uint8_t result = boltffi_method_enum_demo_mode_code((___Mode)receiver);",
        "return (jbyte)result;",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
}

#[test]
fn jni_bridge_renders_async_enum_methods() {
    let files = files(
        r#"
        #[repr(u8)]
        #[data]
        pub enum Mode {
            Fast = 1,
            Slow = 2,
        }

        #[data(impl)]
        impl Mode {
            pub async fn compute(&self) -> u32 {
                7
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
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1method_1enum_1demo_1mode_1compute(JNIEnv *env, jclass cls, jbyte receiver)",
        "RustFutureHandle result = boltffi_method_enum_demo_mode_compute((___Mode)receiver);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1enum_1demo_1mode_1compute_1poll(JNIEnv *env, jclass cls, jlong handle, jlong callback_data)",
        "boltffi_async_method_enum_demo_mode_compute_poll((RustFutureHandle)handle, callback_data, boltffi_jni_continuation_callback);",
        "JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1enum_1demo_1mode_1compute_1complete(JNIEnv *env, jclass cls, jlong handle, jlong out_status)",
        "uint32_t result = boltffi_async_method_enum_demo_mode_compute_complete((RustFutureHandle)handle, (FfiStatus *)out_status);",
        "return (jint)result;",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1enum_1demo_1mode_1compute_1cancel(JNIEnv *env, jclass cls, jlong handle)",
        "boltffi_async_method_enum_demo_mode_compute_cancel((RustFutureHandle)handle);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1async_1method_1enum_1demo_1mode_1compute_1free(JNIEnv *env, jclass cls, jlong handle)",
        "boltffi_async_method_enum_demo_mode_compute_free((RustFutureHandle)handle);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}\n{source}"));
}

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

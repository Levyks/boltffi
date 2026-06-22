use super::files;

#[test]
fn jni_bridge_maps_primitive_direct_vectors_to_java_primitive_arrays() {
    let files = files(
        r#"
            #[export]
            pub fn sum(values: Vec<i32>) -> i32 {
                values.into_iter().sum()
            }
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1sum(JNIEnv *env, jclass cls, jintArray values)"));
    assert!(source.contains("jint *__boltffi_values_ptr = NULL;"));
    assert!(source.contains("jint __boltffi_values_stack[8];"));
    assert!(source.contains("bool __boltffi_values_needs_release = false;"));
    assert!(source.contains("if (__boltffi_values_len <= (jsize)8)"));
    assert!(source.contains(
        "(*env)->GetIntArrayRegion(env, values, 0, __boltffi_values_len, __boltffi_values_stack);"
    ));
    assert!(source.contains("__boltffi_values_ptr = __boltffi_values_stack;"));
    assert!(
        source.contains("__boltffi_values_ptr = (*env)->GetIntArrayElements(env, values, NULL);")
    );
    assert!(source.contains("int32_t result = boltffi_function_demo_sum((const int32_t *)__boltffi_values_ptr, (uintptr_t)__boltffi_values_len);"));
    assert!(source.contains("if (__boltffi_values_needs_release)"));
    assert!(source.contains(
        "(*env)->ReleaseIntArrayElements(env, values, __boltffi_values_ptr, JNI_ABORT);"
    ));
}

#[test]
fn jni_bridge_maps_direct_record_vectors_to_java_byte_arrays() {
    let files = files(
        r#"
            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[export]
            pub fn count(values: Vec<Point>) -> u32 {
                values.len() as u32
            }
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1count(JNIEnv *env, jclass cls, jbyteArray values)"));
    assert!(source.contains("jbyte *__boltffi_values_ptr = NULL;"));
    assert!(
        source.contains("__boltffi_values_ptr = (*env)->GetByteArrayElements(env, values, NULL);")
    );
    assert!(!source.contains("__boltffi_values_stack"));
    assert!(!source.contains("GetByteArrayRegion(env, values, 0, __boltffi_values_len"));
    assert!(source.contains("uint32_t result = boltffi_function_demo_count((const uint8_t *)__boltffi_values_ptr, (uintptr_t)__boltffi_values_len);"));
    assert!(source.contains(
        "(*env)->ReleaseByteArrayElements(env, values, __boltffi_values_ptr, JNI_ABORT);"
    ));
}

#[test]
fn jni_bridge_maps_callback_direct_vectors_to_java_primitive_arrays() {
    let files = files(
        r#"
            #[export]
            pub trait Collector {
                fn on_values(&self, values: Vec<i32>);
            }
            "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(source.contains("static void ___CollectorVTable_on_values(uint64_t handle, const int32_t *values_ptr, uintptr_t values_len)"));
    assert!(source.contains("jintArray values = NULL;"));
    assert!(source.contains("values = (*env)->NewIntArray(env, (jsize)values_len);"));
    assert!(source.contains(
        "(*env)->SetIntArrayRegion(env, values, 0, (jsize)values_len, (const jint *)values_ptr);"
    ));
    assert!(source.contains("(*env)->CallStaticVoidMethod(env, g____CollectorVTable_class, g____CollectorVTable_on_values_method, (jlong)handle, values);"));
    assert!(source.contains("(*env)->DeleteLocalRef(env, values);"));
}

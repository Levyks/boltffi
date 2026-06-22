use super::files;

#[test]
fn jni_bridge_renders_accessor_constants() {
    let files = files(
        r#"
        #[export]
        pub const ANSWER: u32 = 42;

        #[export]
        pub const MAGIC: &'static [u8] = b"ffi";
        "#,
    );
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    [
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1const_1demo_1magic(JNIEnv *env, jclass cls)",
        "FfiBuf_u8 result = boltffi_const_demo_magic();",
        "return boltffi_jni_buffer_to_byte_array(env, result);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));

    assert!(!source.contains("boltffi_const_demo_answer"));
}

use super::files;

#[test]
fn jni_bridge_renders_stream_protocol_functions() {
    let files = files(
        r#"
        use std::sync::Arc;
        use boltffi::EventSubscription;

        #[repr(C)]
        #[data]
        pub struct Point {
            pub x: f64,
            pub y: f64,
        }

        pub struct Engine;

        #[export(single_threaded)]
        impl Engine {
            #[ffi_stream(item = Point, mode = "batch")]
            pub fn points(&self) -> Arc<EventSubscription<Point>> {
                todo!()
            }

            #[ffi_stream(item = String)]
            pub fn names(&self) -> Arc<EventSubscription<String>> {
                todo!()
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
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1points_1subscribe(JNIEnv *env, jclass cls, jlong receiver)",
        "uint64_t result = boltffi_stream_demo_engine_points_subscribe(receiver);",
        "JNIEXPORT jlong JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1points_1pop_1batch(JNIEnv *env, jclass cls, jlong subscription, jlong output_ptr, jlong output_capacity)",
        "uintptr_t result = boltffi_stream_demo_engine_points_pop_batch(subscription, (___Point *)output_ptr, output_capacity);",
        "JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1points_1wait(JNIEnv *env, jclass cls, jlong subscription, jint timeout_milliseconds)",
        "WaitResult result = boltffi_stream_demo_engine_points_wait(subscription, timeout_milliseconds);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1points_1poll(JNIEnv *env, jclass cls, jlong subscription, jlong callback_data)",
        "boltffi_stream_demo_engine_points_poll(subscription, callback_data, boltffi_jni_continuation_callback);",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1points_1unsubscribe(JNIEnv *env, jclass cls, jlong subscription)",
        "JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1points_1free(JNIEnv *env, jclass cls, jlong subscription)",
        "JNIEXPORT jbyteArray JNICALL Java_com_boltffi_demo_Native_boltffi_1stream_1demo_1engine_1names_1pop_1batch(JNIEnv *env, jclass cls, jlong subscription, jlong max_count)",
        "FfiBuf_u8 result = boltffi_stream_demo_engine_names_pop_batch(subscription, max_count);",
        "return boltffi_jni_buffer_to_byte_array(env, result);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
}

static void boltffi_jni_throw_error_buffer(JNIEnv *env, FfiBuf_u8 buffer) {
    {{ free_buffer }}(buffer);
    boltffi_jni_throw_runtime(env, "BoltFFI call failed");
}

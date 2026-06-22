static jmethodID boltffi_jni_continuation_method = NULL;

static bool boltffi_jni_continuation_load(JNIEnv *env) {
    boltffi_jni_continuation_method = (*env)->GetStaticMethodID(env, boltffi_jni_native_class, "boltffiFutureContinuationCallback", "(JB)V");
    if (boltffi_jni_continuation_method == NULL) {
        return false;
    }
    return true;
}

static void boltffi_jni_continuation_unload(JNIEnv *env) {
    (void)env;
    boltffi_jni_continuation_method = NULL;
}

static void boltffi_jni_continuation_callback(uint64_t handle, int8_t poll_result) {
    if (boltffi_jni_vm == NULL || boltffi_jni_native_class == NULL || boltffi_jni_continuation_method == NULL) {
        return;
    }
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
        return;
    }
    (*env)->CallStaticVoidMethod(env, boltffi_jni_native_class, boltffi_jni_continuation_method, (jlong)handle, (jbyte)poll_result);
    boltffi_jni_clear_exception(env);
    boltffi_jni_exit(env, attached);
}

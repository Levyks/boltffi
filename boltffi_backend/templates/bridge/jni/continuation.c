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
    jint env_status = (*boltffi_jni_vm)->GetEnv(boltffi_jni_vm, (void **)&env, JNI_VERSION_1_6);
    if (env_status == JNI_EDETACHED) {
        if (boltffi_jni_attach_current_thread(boltffi_jni_vm, &env) != JNI_OK) {
            return;
        }
        attached = 1;
    } else if (env_status != JNI_OK) {
        return;
    }
    (*env)->CallStaticVoidMethod(env, boltffi_jni_native_class, boltffi_jni_continuation_method, (jlong)handle, (jbyte)poll_result);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->ExceptionClear(env);
    }
    if (attached) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}

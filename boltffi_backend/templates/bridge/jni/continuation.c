static JavaVM *boltffi_jni_vm = NULL;
static jclass boltffi_jni_native_class = NULL;
static jmethodID boltffi_jni_continuation_method = NULL;

static jint boltffi_jni_attach_current_thread(JavaVM *vm, JNIEnv **env) {
#if defined(__ANDROID__)
    return (*vm)->AttachCurrentThread(vm, env, NULL);
#else
    return (*vm)->AttachCurrentThread(vm, (void **)env, NULL);
#endif
}

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return JNI_ERR;
    }
    jclass local_class = (*env)->FindClass(env, {{ class_name }});
    if (local_class == NULL) {
        return JNI_ERR;
    }
    boltffi_jni_native_class = (*env)->NewGlobalRef(env, local_class);
    (*env)->DeleteLocalRef(env, local_class);
    if (boltffi_jni_native_class == NULL) {
        return JNI_ERR;
    }
    boltffi_jni_continuation_method = (*env)->GetStaticMethodID(env, boltffi_jni_native_class, "boltffiFutureContinuationCallback", "(JB)V");
    if (boltffi_jni_continuation_method == NULL) {
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
    boltffi_jni_vm = vm;
    return JNI_VERSION_1_6;
}

JNIEXPORT void JNICALL JNI_OnUnload(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) == JNI_OK && boltffi_jni_native_class != NULL) {
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
    }
    boltffi_jni_vm = NULL;
    boltffi_jni_native_class = NULL;
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

static JavaVM *boltffi_jni_vm = NULL;
static jclass boltffi_jni_native_class = NULL;

static jint boltffi_jni_attach_current_thread(JavaVM *vm, JNIEnv **env) {
#if defined(__ANDROID__)
    return (*vm)->AttachCurrentThread(vm, env, NULL);
#else
    return (*vm)->AttachCurrentThread(vm, (void **)env, NULL);
#endif
}

static bool boltffi_jni_enter(JNIEnv **env, int *attached) {
    if (boltffi_jni_vm == NULL) {
        return false;
    }
    *attached = 0;
    jint env_status = (*boltffi_jni_vm)->GetEnv(boltffi_jni_vm, (void **)env, JNI_VERSION_1_6);
    if (env_status == JNI_EDETACHED) {
        if (boltffi_jni_attach_current_thread(boltffi_jni_vm, env) != JNI_OK) {
            return false;
        }
        *attached = 1;
        return true;
    }
    return env_status == JNI_OK;
}

static void boltffi_jni_exit(int attached) {
    if (attached) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}

static bool boltffi_jni_clear_exception(JNIEnv *env) {
    if (!(*env)->ExceptionCheck(env)) {
        return false;
    }
    (*env)->ExceptionClear(env);
    return true;
}

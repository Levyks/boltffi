static JavaVM *boltffi_jni_vm = NULL;
static jclass boltffi_jni_native_class = NULL;

#define BOLTFFI_JNI_LOCAL_FRAME_CAPACITY 64

static jint boltffi_jni_attach_current_thread(JavaVM *vm, JNIEnv **env) {
#if defined(__ANDROID__)
    return (*vm)->AttachCurrentThread(vm, env, NULL);
#else
    return (*vm)->AttachCurrentThread(vm, (void **)env, NULL);
#endif
}

#if defined(__ANDROID__)
static pthread_key_t boltffi_jni_env_key;
static pthread_once_t boltffi_jni_env_key_once = PTHREAD_ONCE_INIT;
static int boltffi_jni_env_key_status = 0;
static char boltffi_jni_tls_attached_marker;

static void boltffi_jni_android_env_destructor(void *value) {
    if (value != NULL && boltffi_jni_vm != NULL) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}

static void boltffi_jni_android_env_key_init(void) {
    boltffi_jni_env_key_status =
        pthread_key_create(&boltffi_jni_env_key, boltffi_jni_android_env_destructor);
}

static jint boltffi_jni_android_attach_cached(JavaVM *vm, JNIEnv **env, int *attached) {
    *attached = 0;

    if (pthread_once(&boltffi_jni_env_key_once, boltffi_jni_android_env_key_init) != 0 ||
        boltffi_jni_env_key_status != 0) {
        jint result = boltffi_jni_attach_current_thread(vm, env);
        if (result == JNI_OK) {
            *attached = 1;
        }
        return result;
    }

    jint result = (*vm)->AttachCurrentThreadAsDaemon(vm, env, NULL);
    if (result != JNI_OK) {
        return result;
    }

    if (pthread_setspecific(boltffi_jni_env_key, &boltffi_jni_tls_attached_marker) != 0) {
        (*vm)->DetachCurrentThread(vm);
        *env = NULL;
        return JNI_ERR;
    }

    return JNI_OK;
}
#endif

static bool boltffi_jni_clear_exception(JNIEnv *env) {
    if (!(*env)->ExceptionCheck(env)) {
        return false;
    }
    (*env)->ExceptionClear(env);
    return true;
}

static bool boltffi_jni_enter(JNIEnv **env, int *attached) {
    if (boltffi_jni_vm == NULL) {
        return false;
    }
    *env = NULL;
    *attached = 0;
    jint env_status = (*boltffi_jni_vm)->GetEnv(boltffi_jni_vm, (void **)env, JNI_VERSION_1_6);
    if (env_status == JNI_EDETACHED) {
#if defined(__ANDROID__)
        if (boltffi_jni_android_attach_cached(boltffi_jni_vm, env, attached) != JNI_OK) {
            return false;
        }
#else
        if (boltffi_jni_attach_current_thread(boltffi_jni_vm, env) != JNI_OK) {
            return false;
        }
        *attached = 1;
#endif
    } else if (env_status != JNI_OK) {
        return false;
    }

#if defined(__ANDROID__)
    JNIEnv *callback_env = *env;
    if ((*callback_env)->PushLocalFrame(callback_env, BOLTFFI_JNI_LOCAL_FRAME_CAPACITY) != JNI_OK) {
        boltffi_jni_clear_exception(callback_env);
        if (*attached) {
            (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
            *attached = 0;
        }
        return false;
    }
#endif

    return true;
}

static void boltffi_jni_exit(JNIEnv *env, int attached) {
#if defined(__ANDROID__)
    if (env != NULL) {
        (*env)->PopLocalFrame(env, NULL);
        boltffi_jni_clear_exception(env);
    }
#else
    (void)env;
#endif
    if (attached) {
        (*boltffi_jni_vm)->DetachCurrentThread(boltffi_jni_vm);
    }
}

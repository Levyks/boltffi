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
{%- if uses_continuations %}
    if (!boltffi_jni_continuation_load(env)) {
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
{%- endif %}
{%- for callback in callbacks %}
    if (!{{ callback.load }}(env)) {
{%- for cleanup in callbacks %}
        {{ cleanup.unload }}(env);
{%- endfor %}
{%- if uses_continuations %}
        boltffi_jni_continuation_unload(env);
{%- endif %}
        (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        boltffi_jni_native_class = NULL;
        return JNI_ERR;
    }
{%- endfor %}
    boltffi_jni_vm = vm;
    return JNI_VERSION_1_6;
}

JNIEXPORT void JNICALL JNI_OnUnload(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) == JNI_OK) {
{%- if uses_continuations %}
        boltffi_jni_continuation_unload(env);
{%- endif %}
{%- for callback in callbacks %}
        {{ callback.unload }}(env);
{%- endfor %}
        if (boltffi_jni_native_class != NULL) {
            (*env)->DeleteGlobalRef(env, boltffi_jni_native_class);
        }
    }
    boltffi_jni_vm = NULL;
    boltffi_jni_native_class = NULL;
}

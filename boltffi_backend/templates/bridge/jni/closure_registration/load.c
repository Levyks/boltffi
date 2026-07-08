static bool {{ closure.load }}(JNIEnv *env) {
    if (!boltffi_jni_lookup_global_class(env, {{ closure.class }}, &{{ closure.global_class }})) {
        return false;
    }
    if (!boltffi_jni_lookup_static_method(env, {{ closure.global_class }}, {{ closure.class }}, "call", {{ closure.method_signature }}, &{{ closure.call_method }})) {
        goto fail;
    }
    if (!boltffi_jni_lookup_static_method(env, {{ closure.global_class }}, {{ closure.class }}, "free", "(J)V", &{{ closure.free_method }})) {
        goto fail;
    }
    return true;
fail:
    (*env)->DeleteGlobalRef(env, {{ closure.global_class }});
    {{ closure.global_class }} = NULL;
    return false;
}

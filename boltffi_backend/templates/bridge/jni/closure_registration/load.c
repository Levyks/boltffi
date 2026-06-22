static bool {{ closure.load }}(JNIEnv *env) {
    jclass local_class = (*env)->FindClass(env, {{ closure.class }});
    if (local_class == NULL) {
        return false;
    }
    {{ closure.global_class }} = (*env)->NewGlobalRef(env, local_class);
    (*env)->DeleteLocalRef(env, local_class);
    if ({{ closure.global_class }} == NULL) {
        return false;
    }
    {{ closure.call_method }} = (*env)->GetStaticMethodID(env, {{ closure.global_class }}, "call", {{ closure.method_signature }});
    if ({{ closure.call_method }} == NULL) {
        goto fail;
    }
    {{ closure.free_method }} = (*env)->GetStaticMethodID(env, {{ closure.global_class }}, "free", "(J)V");
    if ({{ closure.free_method }} == NULL) {
        goto fail;
    }
    return true;
fail:
    (*env)->DeleteGlobalRef(env, {{ closure.global_class }});
    {{ closure.global_class }} = NULL;
    return false;
}

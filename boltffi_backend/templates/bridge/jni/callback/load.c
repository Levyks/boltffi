static bool {{ callback.load }}(JNIEnv *env) {
    jclass local_class = (*env)->FindClass(env, {{ callback.class }});
    if (local_class == NULL) {
        return false;
    }
    {{ callback.global_class }} = (*env)->NewGlobalRef(env, local_class);
    (*env)->DeleteLocalRef(env, local_class);
    if ({{ callback.global_class }} == NULL) {
        return false;
    }
    {{ callback.free_method }} = (*env)->GetStaticMethodID(env, {{ callback.global_class }}, "free", "(J)V");
    if ({{ callback.free_method }} == NULL) {
        goto fail;
    }
    {{ callback.clone_method }} = (*env)->GetStaticMethodID(env, {{ callback.global_class }}, "clone", "(J)J");
    if ({{ callback.clone_method }} == NULL) {
        goto fail;
    }
{%- for method in callback.methods %}
    {{ method.method_id }} = (*env)->GetStaticMethodID(env, {{ callback.global_class }}, "{{ method.method }}", {{ method.signature }});
    if ({{ method.method_id }} == NULL) {
        goto fail;
    }
{%- endfor %}
    {{ callback.register }}(&{{ callback.vtable }});
    return true;
fail:
    (*env)->DeleteGlobalRef(env, {{ callback.global_class }});
    {{ callback.global_class }} = NULL;
    return false;
}

static bool {{ callback.load }}(JNIEnv *env) {
    if (!boltffi_jni_lookup_global_class(env, {{ callback.class }}, &{{ callback.global_class }})) {
        return false;
    }
    if (!boltffi_jni_lookup_static_method(env, {{ callback.global_class }}, {{ callback.class }}, "free", "(J)V", &{{ callback.free_method }})) {
        goto fail;
    }
    if (!boltffi_jni_lookup_static_method(env, {{ callback.global_class }}, {{ callback.class }}, "clone", "(J)J", &{{ callback.clone_method }})) {
        goto fail;
    }
{%- for method in callback.methods %}
    if (!boltffi_jni_lookup_static_method(env, {{ callback.global_class }}, {{ callback.class }}, "{{ method.method }}", {{ method.signature }}, &{{ method.method_id }})) {
        goto fail;
    }
{%- endfor %}
{%- for method in callback.handle_methods %}
{%- match method.completion %}
{%- when Some with (completion) %}
    if (!boltffi_jni_lookup_static_method(env, {{ callback.global_class }}, {{ callback.class }}, "{{ completion.success_method }}", {{ completion.success_signature }}, &{{ completion.success_method_id }})) {
        goto fail;
    }
    if (!boltffi_jni_lookup_static_method(env, {{ callback.global_class }}, {{ callback.class }}, "{{ completion.failure_method }}", {{ completion.failure_signature }}, &{{ completion.failure_method_id }})) {
        goto fail;
    }
{%- when None %}
{%- endmatch %}
{%- endfor %}
    {{ callback.register }}(&{{ callback.vtable }});
    return true;
fail:
    (*env)->DeleteGlobalRef(env, {{ callback.global_class }});
    {{ callback.global_class }} = NULL;
    return false;
}

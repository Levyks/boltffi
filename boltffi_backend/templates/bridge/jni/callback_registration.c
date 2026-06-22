{%- for callback in callbacks %}

static jclass {{ callback.global_class }} = NULL;
static jmethodID {{ callback.free_method }} = NULL;
static jmethodID {{ callback.clone_method }} = NULL;
{%- for method in callback.methods %}
static jmethodID {{ method.method_id }} = NULL;
{%- endfor %}

static void {{ callback.free }}(uint64_t handle) {
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
        return;
    }
    (*env)->CallStaticVoidMethod(env, {{ callback.global_class }}, {{ callback.free_method }}, (jlong)handle);
    boltffi_jni_clear_exception(env);
    boltffi_jni_exit(attached);
}

static uint64_t {{ callback.clone }}(uint64_t handle) {
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
        return 0;
    }
    jlong result = (*env)->CallStaticLongMethod(env, {{ callback.global_class }}, {{ callback.clone_method }}, (jlong)handle);
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(attached);
        return 0;
    }
    boltffi_jni_exit(attached);
    return (uint64_t)result;
}

{%- for method in callback.methods %}

static {{ method.c_return_type }} {{ method.function }}({% for parameter in method.parameters %}{{ parameter.c_type }} {{ parameter.name }}{% if !loop.last %}, {% endif %}{% endfor %}) {
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
{%- if method.returns_void %}
        return;
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- if method.returns_void %}
    (*env)->CallStaticVoidMethod(env, {{ callback.global_class }}, {{ method.method_id }}{% for parameter in method.parameters %}, ({{ parameter.jni_type }}){{ parameter.name }}{% endfor %});
    boltffi_jni_clear_exception(env);
    boltffi_jni_exit(attached);
{%- else %}
    {{ method.c_return_type }} result = ({{ method.c_return_type }})(*env)->CallStatic{{ method.call_method_suffix }}Method(env, {{ callback.global_class }}, {{ method.method_id }}{% for parameter in method.parameters %}, ({{ parameter.jni_type }}){{ parameter.name }}{% endfor %});
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(attached);
        return {{ method.failure_value }};
    }
    boltffi_jni_exit(attached);
    return result;
{%- endif %}
}
{%- endfor %}

static {{ callback.vtable_type }} {{ callback.vtable }} = {
    .free = {{ callback.free }},
    .clone = {{ callback.clone }},
{%- for method in callback.methods %}
    .{{ method.method }} = {{ method.function }},
{%- endfor %}
};

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

static void {{ callback.unload }}(JNIEnv *env) {
    if ({{ callback.global_class }} != NULL) {
        (*env)->DeleteGlobalRef(env, {{ callback.global_class }});
    }
    {{ callback.global_class }} = NULL;
    {{ callback.free_method }} = NULL;
    {{ callback.clone_method }} = NULL;
{%- for method in callback.methods %}
    {{ method.method_id }} = NULL;
{%- endfor %}
}
{%- endfor %}

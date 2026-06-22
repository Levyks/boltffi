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

static {{ method.c_return_type }} {{ method.function }}({% for parameter in method.c_parameters %}{{ parameter.declaration }}{% if !loop.last %}, {% endif %}{% endfor %}) {
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
{%- if method.returns_void %}
        return;
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- for bytes in method.byte_arrays %}
    jbyteArray {{ bytes.name }} = boltffi_jni_bytes_to_byte_array(env, {{ bytes.pointer }}, {{ bytes.length }});
    if ({{ bytes.name }} == NULL) {
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(attached);
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
{%- if method.returns_void %}
        return;
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- endfor %}
{%- for vector in method.direct_vectors %}
    {{ vector.array_type }} {{ vector.array }} = NULL;
{%- endfor %}
{%- for vector in method.direct_vectors %}
{% include "bridge/jni/callback_direct_vector_argument.c" %}
{%- endfor %}
{%- for callback_handle in method.callback_handles %}
    jlong {{ callback_handle.handle }} = 0;
{%- endfor %}
{%- for closure_handle in method.closure_handles %}
    jlong {{ closure_handle.handle }} = 0;
{%- endfor %}
{%- for record in method.record_arrays %}
    jbyteArray {{ record.array }} = boltffi_jni_record_to_byte_array(env, &{{ record.parameter }}, (uintptr_t)sizeof({{ record.parameter }}));
    if ({{ record.array }} == NULL) {
{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for vector in method.direct_vectors %}
        if ({{ vector.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ vector.array }});
        }
{%- endfor %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(attached);
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
{%- if method.returns_void %}
        return;
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- endfor %}
{%- for callback_handle in method.callback_handles %}
    {{ callback_handle.handle }} = boltffi_jni_callback_handle_new_owned(env, {{ callback_handle.parameter }});
    if ((*env)->ExceptionCheck(env)) {
{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for vector in method.direct_vectors %}
        if ({{ vector.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ vector.array }});
        }
{%- endfor %}
{%- for record in method.record_arrays %}
        (*env)->DeleteLocalRef(env, {{ record.array }});
{%- endfor %}
{%- for handle in method.callback_handles %}
        boltffi_jni_callback_handle_release(boltffi_jni_callback_handle_ref({{ handle.handle }}));
{%- endfor %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(attached);
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
{%- if method.returns_void %}
        return;
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- endfor %}
{%- for closure_handle in method.closure_handles %}
    {{ closure_handle.handle }} = {{ closure_handle.handle_new }}(env, {{ closure_handle.call }}, (void *){{ closure_handle.context }}, {{ closure_handle.release }});
    if ((*env)->ExceptionCheck(env)) {
{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for vector in method.direct_vectors %}
        if ({{ vector.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ vector.array }});
        }
{%- endfor %}
{%- for record in method.record_arrays %}
        (*env)->DeleteLocalRef(env, {{ record.array }});
{%- endfor %}
{%- for handle in method.callback_handles %}
        boltffi_jni_callback_handle_release(boltffi_jni_callback_handle_ref({{ handle.handle }}));
{%- endfor %}
{%- for handle in method.closure_handles %}
        {{ handle.handle_release }}({{ handle.handle }});
{%- endfor %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(attached);
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
{%- if method.returns_void %}
        return;
{%- else %}
        return {{ method.failure_value }};
{%- endif %}
    }
{%- endfor %}
{%- if method.returns_void %}
    (*env)->CallStaticVoidMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- for bytes in method.byte_arrays %}
    (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for vector in method.direct_vectors %}
    if ({{ vector.array }} != NULL) {
        (*env)->DeleteLocalRef(env, {{ vector.array }});
    }
{%- endfor %}
{%- for record in method.record_arrays %}
    (*env)->DeleteLocalRef(env, {{ record.array }});
{%- endfor %}
    if (boltffi_jni_clear_exception(env)) {
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
    }
    boltffi_jni_exit(attached);
{%- else %}
{%- if method.returns_byte_array %}
    jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- else %}
    {{ method.c_return_type }} result = ({{ method.c_return_type }})(*env)->CallStatic{{ method.call_method_suffix }}Method(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- endif %}
{%- for bytes in method.byte_arrays %}
    (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for vector in method.direct_vectors %}
    if ({{ vector.array }} != NULL) {
        (*env)->DeleteLocalRef(env, {{ vector.array }});
    }
{%- endfor %}
{%- for record in method.record_arrays %}
    (*env)->DeleteLocalRef(env, {{ record.array }});
{%- endfor %}
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(attached);
        return {{ method.failure_value }};
    }
{%- if method.returns_bytes %}
    {{ method.c_return_type }} result = boltffi_jni_byte_array_to_buffer(env, __boltffi_return_array);
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(attached);
        return {{ method.failure_value }};
    }
{%- else if method.returns_record %}
    {{ method.c_return_type }} result = {0};
    if (!boltffi_jni_read_record(env, __boltffi_return_array, (uintptr_t)sizeof(result), &result)) {
        (*env)->DeleteLocalRef(env, __boltffi_return_array);
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(attached);
        return {{ method.failure_value }};
    }
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
{%- endif %}
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

    if ({{ vector.length }} > (uintptr_t)INT32_MAX) {
        boltffi_jni_throw_runtime(env, "BoltFFI vector argument too large for Java array");
{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for cleanup in method.direct_vectors %}
        if ({{ cleanup.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ cleanup.array }});
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
    {{ vector.array }} = (*env)->{{ vector.new_array }}(env, (jsize){{ vector.length }});
    if ({{ vector.array }} == NULL) {
{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for cleanup in method.direct_vectors %}
        if ({{ cleanup.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ cleanup.array }});
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
    (*env)->{{ vector.set_region }}(env, {{ vector.array }}, 0, (jsize){{ vector.length }}, (const {{ vector.element_type }} *){{ vector.pointer }});
    if ((*env)->ExceptionCheck(env)) {
{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for cleanup in method.direct_vectors %}
        if ({{ cleanup.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ cleanup.array }});
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

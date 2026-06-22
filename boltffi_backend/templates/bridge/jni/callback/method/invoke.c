{%- if method.returns_void %}
    (*env)->CallStaticVoidMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{% include "bridge/jni/callback/method/cleanup.c" %}
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
{% include "bridge/jni/callback/method/cleanup.c" %}
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

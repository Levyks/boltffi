{%- if method.returns_void %}
    (void)env;
    {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/writebacks.c" %}
    return;
{%- else if method.checks_status %}
    {{ method.c_result_type }} status = {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
    if (status.code != 0) {
        boltffi_jni_throw_status(env, status);
        return;
    }
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.success_out.is_some() %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, {{ method.return_value }});
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &{{ method.return_value }}, (uintptr_t)sizeof({{ method.return_value }}));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- else %}
    return;
{%- endif %}
{%- else if method.checks_error_buffer %}
    {{ method.c_result_type }} error = {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
    if (error.ptr != NULL || error.len != 0) {
        boltffi_jni_throw_error_buffer(env, error);
{%- include "bridge/jni/method/error_return_nested.c" %}
    }
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, {{ method.return_value }});
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &{{ method.return_value }}, (uintptr_t)sizeof({{ method.return_value }}));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- else %}
    (void)env;
    {{ method.c_result_type }} result = {{ method.c_function }}({{ method.arguments }});
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/writebacks.c" %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, result);
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- endif %}

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
    return;
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

{%- for parameter in method.record_arrays %}
    if (!boltffi_jni_read_record(env, {{ parameter.name }}, (uintptr_t)sizeof({{ parameter.c_type }}), &{{ parameter.local }})) {
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
    }
{%- endfor %}

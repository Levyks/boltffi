{%- for parameter in method.record_arrays %}
    if (!boltffi_jni_read_record(env, {{ parameter.name }}, (uintptr_t)sizeof({{ parameter.c_type }}), &{{ parameter.local }})) {
        goto __boltffi_error;
    }
{%- endfor %}

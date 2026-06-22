{%- for record in method.record_arrays %}
    jbyteArray {{ record.array }} = boltffi_jni_record_to_byte_array(env, &{{ record.parameter }}, (uintptr_t)sizeof({{ record.parameter }}));
    if ({{ record.array }} == NULL) {
{% include "bridge/jni/callback/method/cleanup.c" %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }
{%- endfor %}

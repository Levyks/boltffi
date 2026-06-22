{%- for bytes in method.byte_arrays %}
    jbyteArray {{ bytes.name }} = boltffi_jni_bytes_to_byte_array(env, {{ bytes.pointer }}, {{ bytes.length }});
    if ({{ bytes.name }} == NULL) {
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }
{%- endfor %}

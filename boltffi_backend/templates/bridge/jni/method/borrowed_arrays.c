{%- for parameter in method.borrowed_arrays %}
    if ({{ parameter.name }} == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
    }
    {{ parameter.length }} = (*env)->GetArrayLength(env, {{ parameter.name }});
    {{ parameter.pointer }} = (*env)->{{ parameter.getter }}(env, {{ parameter.name }}, NULL);
    if ({{ parameter.pointer }} == NULL) {
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
    }
{%- endfor %}

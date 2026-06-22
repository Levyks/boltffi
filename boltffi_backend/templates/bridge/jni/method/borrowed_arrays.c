{%- for parameter in method.borrowed_arrays %}
    if ({{ parameter.name }} == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI array argument was null");
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
    }
    {{ parameter.length }} = (*env)->GetArrayLength(env, {{ parameter.name }});
{%- if let Some(stack) = parameter.stack_copy %}
    if ({{ parameter.length }} <= (jsize){{ stack.max_len }}) {
        (*env)->{{ stack.region_getter }}(env, {{ parameter.name }}, 0, {{ parameter.length }}, {{ stack.storage }});
        if ((*env)->ExceptionCheck(env)) {
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
        }
        {{ parameter.pointer }} = {{ stack.storage }};
    } else {
        {{ parameter.pointer }} = (*env)->{{ parameter.getter }}(env, {{ parameter.name }}, NULL);
        if ({{ parameter.pointer }} == NULL) {
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
        }
        {{ stack.needs_release }} = true;
    }
{%- else %}
    {{ parameter.pointer }} = (*env)->{{ parameter.getter }}(env, {{ parameter.name }}, NULL);
    if ({{ parameter.pointer }} == NULL) {
{% include "bridge/jni/method/cleanup_arrays.c" %}
{% include "bridge/jni/method/error_return.c" %}
    }
{%- endif %}
{%- endfor %}

    (*env)->CallStaticVoidMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{% include "bridge/jni/callback/method/cleanup.c" %}
    if (boltffi_jni_clear_exception(env)) {
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
    }
    boltffi_jni_exit(env, attached);

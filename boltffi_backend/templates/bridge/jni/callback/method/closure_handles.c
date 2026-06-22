{%- for closure_handle in method.closure_handles %}
    {{ closure_handle.handle }} = {{ closure_handle.handle_new }}(env, {{ closure_handle.call }}, (void *){{ closure_handle.context }}, {{ closure_handle.release }});
    if ((*env)->ExceptionCheck(env)) {
{% include "bridge/jni/callback/method/cleanup.c" %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }
{%- endfor %}

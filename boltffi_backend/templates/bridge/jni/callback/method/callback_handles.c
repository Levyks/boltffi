{%- for callback_handle in method.callback_handles %}
    {{ callback_handle.handle }} = boltffi_jni_callback_handle_new_owned(env, {{ callback_handle.parameter }});
    if ((*env)->ExceptionCheck(env)) {
{% include "bridge/jni/callback/method/cleanup.c" %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }
{%- endfor %}

{% include "bridge/jni/callback/method/invoke/raw_return.c" %}
{% include "bridge/jni/callback/method/cleanup.c" %}
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(env, attached);
        return {{ method.failure_value }};
    }
{% include "bridge/jni/callback/method/invoke/return.c" %}
    boltffi_jni_exit(env, attached);
    return result;

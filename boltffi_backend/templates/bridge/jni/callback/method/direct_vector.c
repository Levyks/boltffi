    if ({{ vector.length }} > (uintptr_t)INT32_MAX) {
        boltffi_jni_throw_runtime(env, "BoltFFI vector argument too large for Java array");
{% include "bridge/jni/callback/method/cleanup.c" %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }
    {{ vector.array }} = (*env)->{{ vector.new_array }}(env, (jsize){{ vector.length }});
    if ({{ vector.array }} == NULL) {
{% include "bridge/jni/callback/method/cleanup.c" %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }
    (*env)->{{ vector.set_region }}(env, {{ vector.array }}, 0, (jsize){{ vector.length }}, (const {{ vector.element_type }} *){{ vector.pointer }});
    if ((*env)->ExceptionCheck(env)) {
{% include "bridge/jni/callback/method/cleanup.c" %}
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
{% include "bridge/jni/callback/method/fail.c" %}
    }

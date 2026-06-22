static {{ method.c_return_type }} {{ method.function }}({% for parameter in method.c_parameters %}{{ parameter.declaration }}{% if !loop.last %}, {% endif %}{% endfor %}) {
    JNIEnv *env = NULL;
    int attached = 0;
    if (!boltffi_jni_enter(&env, &attached)) {
{% include "bridge/jni/callback/method/fail.c" %}
    }
{% include "bridge/jni/callback/method/byte_arrays.c" %}
{% include "bridge/jni/callback/method/direct_vectors.c" %}
{% include "bridge/jni/callback/method/handles.c" %}
{% include "bridge/jni/callback/method/record_arrays.c" %}
{% include "bridge/jni/callback/method/callback_handles.c" %}
{% include "bridge/jni/callback/method/closure_handles.c" %}
{% include "bridge/jni/callback/method/invoke.c" %}
}

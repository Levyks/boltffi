JNIEXPORT {{ method.return_type }} JNICALL {{ method.symbol }}(JNIEnv *env, jclass cls{% for parameter in method.parameters %}, {{ parameter.ty }} {{ parameter.name }}{% endfor %}) {
    (void)cls;
{% include "bridge/jni/method/locals.c" %}
{% include "bridge/jni/method/borrowed_arrays.c" %}
{% include "bridge/jni/method/records.c" %}
{% include "bridge/jni/method/call.c" %}
}

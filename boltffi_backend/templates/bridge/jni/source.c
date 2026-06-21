#include <jni.h>
#include <stdint.h>
#include <stdbool.h>

#include {{ c_header }}

{%- if checks_status %}

static void boltffi_jni_throw_runtime(JNIEnv *env, const char *message) {
    jclass exception_class = (*env)->FindClass(env, "java/lang/RuntimeException");
    if (exception_class == NULL) {
        return;
    }
    (*env)->ThrowNew(env, exception_class, message);
    (*env)->DeleteLocalRef(env, exception_class);
}

static void boltffi_jni_throw_status(JNIEnv *env, FfiStatus status) {
    if (status.code != 0) {
        boltffi_jni_throw_runtime(env, "BoltFFI call failed");
    }
}
{%- endif %}

{%- for method in methods %}

JNIEXPORT {{ method.return_type }} JNICALL {{ method.symbol }}(JNIEnv *env, jclass cls{% for parameter in method.parameters %}, {{ parameter.ty }} {{ parameter.name }}{% endfor %}) {
    (void)cls;
{%- if method.returns_void %}
    (void)env;
    {{ method.c_function }}({{ method.arguments }});
{%- else if method.checks_status %}
    {{ method.c_result_type }} status = {{ method.c_function }}({{ method.arguments }});
    boltffi_jni_throw_status(env, status);
{%- else %}
    (void)env;
    {{ method.c_result_type }} result = {{ method.c_function }}({{ method.arguments }});
{%- if method.returns_boolean %}
    return (jboolean)result;
{%- else %}
    return result;
{%- endif %}
{%- endif %}
}
{%- endfor %}

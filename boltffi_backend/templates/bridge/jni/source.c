#include <jni.h>
#include <stdint.h>
#include <stdbool.h>
{%- if uses_byte_arrays %}
#include <limits.h>
{%- endif %}

#include {{ c_header }}

{%- if uses_continuations %}
{% include "bridge/jni/continuation.c" %}
{%- endif %}

{%- if uses_exceptions %}

static void boltffi_jni_throw_runtime(JNIEnv *env, const char *message) {
    jclass exception_class = (*env)->FindClass(env, "java/lang/RuntimeException");
    if (exception_class == NULL) {
        return;
    }
    (*env)->ThrowNew(env, exception_class, message);
    (*env)->DeleteLocalRef(env, exception_class);
}

static void boltffi_jni_throw_illegal_argument(JNIEnv *env, const char *message) {
    jclass exception_class = (*env)->FindClass(env, "java/lang/IllegalArgumentException");
    if (exception_class == NULL) {
        return;
    }
    (*env)->ThrowNew(env, exception_class, message);
    (*env)->DeleteLocalRef(env, exception_class);
}
{%- endif %}

{%- if checks_status %}
static void boltffi_jni_throw_status(JNIEnv *env, FfiStatus status) {
    if (status.code != 0) {
        boltffi_jni_throw_runtime(env, "BoltFFI call failed");
    }
}
{%- endif %}

{%- if uses_byte_arrays %}
static jbyteArray boltffi_jni_buffer_to_byte_array(JNIEnv *env, FfiBuf_u8 buffer) {
    if (buffer.ptr == NULL) {
        if (buffer.len != 0) {
            boltffi_jni_throw_runtime(env, "BoltFFI buffer pointer was null with non-zero length");
        }
        return NULL;
    }
    if (buffer.len > (uintptr_t)INT32_MAX) {
        {{ free_buffer }}(buffer);
        boltffi_jni_throw_runtime(env, "BoltFFI buffer too large for Java byte array");
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)buffer.len);
    if (array == NULL) {
        {{ free_buffer }}(buffer);
        return NULL;
    }
    (*env)->SetByteArrayRegion(env, array, 0, (jsize)buffer.len, (const jbyte *)buffer.ptr);
    {{ free_buffer }}(buffer);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->DeleteLocalRef(env, array);
        return NULL;
    }
    return array;
}
{%- endif %}

{%- if uses_record_arrays %}
static bool boltffi_jni_read_record(JNIEnv *env, jbyteArray array, uintptr_t expected_len, void *output) {
    if (array == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI record byte array argument was null");
        return false;
    }
    jsize len = (*env)->GetArrayLength(env, array);
    if ((uintptr_t)len != expected_len) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI record byte array length did not match the C record size");
        return false;
    }
    (*env)->GetByteArrayRegion(env, array, 0, len, (jbyte *)output);
    return !(*env)->ExceptionCheck(env);
}

static jbyteArray boltffi_jni_record_to_byte_array(JNIEnv *env, const void *record, uintptr_t len) {
    if (len > (uintptr_t)INT32_MAX) {
        boltffi_jni_throw_runtime(env, "BoltFFI record too large for Java byte array");
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)len);
    if (array == NULL) {
        return NULL;
    }
    (*env)->SetByteArrayRegion(env, array, 0, (jsize)len, (const jbyte *)record);
    if ((*env)->ExceptionCheck(env)) {
        (*env)->DeleteLocalRef(env, array);
        return NULL;
    }
    return array;
}
{%- endif %}

{%- for method in methods %}

JNIEXPORT {{ method.return_type }} JNICALL {{ method.symbol }}(JNIEnv *env, jclass cls{% for parameter in method.parameters %}, {{ parameter.ty }} {{ parameter.name }}{% endfor %}) {
    (void)cls;
{%- for parameter in method.byte_arrays %}
    jbyte *{{ parameter.pointer }} = NULL;
    jsize {{ parameter.length }} = 0;
{%- endfor %}
{%- for parameter in method.record_arrays %}
    {{ parameter.c_type }} {{ parameter.local }};
{%- endfor %}
{%- for parameter in method.byte_arrays %}
    if ({{ parameter.name }} == NULL) {
        boltffi_jni_throw_illegal_argument(env, "BoltFFI byte array argument was null");
{%- for cleanup in method.byte_arrays %}
        if ({{ cleanup.pointer }} != NULL) {
            (*env)->ReleaseByteArrayElements(env, {{ cleanup.name }}, {{ cleanup.pointer }}, JNI_ABORT);
        }
{%- endfor %}
{%- if method.returns_void || method.checks_status %}
        return;
{%- else if method.returns_bytes || method.returns_record %}
        return NULL;
{%- else if method.returns_boolean %}
        return JNI_FALSE;
{%- else %}
        return 0;
{%- endif %}
    }
    {{ parameter.length }} = (*env)->GetArrayLength(env, {{ parameter.name }});
    {{ parameter.pointer }} = (*env)->GetByteArrayElements(env, {{ parameter.name }}, NULL);
    if ({{ parameter.pointer }} == NULL) {
{%- for cleanup in method.byte_arrays %}
        if ({{ cleanup.pointer }} != NULL) {
            (*env)->ReleaseByteArrayElements(env, {{ cleanup.name }}, {{ cleanup.pointer }}, JNI_ABORT);
        }
{%- endfor %}
{%- if method.returns_void || method.checks_status %}
        return;
{%- else if method.returns_bytes || method.returns_record %}
        return NULL;
{%- else if method.returns_boolean %}
        return JNI_FALSE;
{%- else %}
        return 0;
{%- endif %}
    }
{%- endfor %}
{%- for parameter in method.record_arrays %}
    if (!boltffi_jni_read_record(env, {{ parameter.name }}, (uintptr_t)sizeof({{ parameter.c_type }}), &{{ parameter.local }})) {
{%- for cleanup in method.byte_arrays %}
        if ({{ cleanup.pointer }} != NULL) {
            (*env)->ReleaseByteArrayElements(env, {{ cleanup.name }}, {{ cleanup.pointer }}, JNI_ABORT);
        }
{%- endfor %}
{%- if method.returns_void || method.checks_status %}
        return;
{%- else if method.returns_bytes || method.returns_record %}
        return NULL;
{%- else if method.returns_boolean %}
        return JNI_FALSE;
{%- else %}
        return 0;
{%- endif %}
    }
{%- endfor %}
{%- if method.returns_void %}
    (void)env;
    {{ method.c_function }}({{ method.arguments }});
{%- for parameter in method.byte_arrays %}
    if ({{ parameter.pointer }} != NULL) {
        (*env)->ReleaseByteArrayElements(env, {{ parameter.name }}, {{ parameter.pointer }}, JNI_ABORT);
    }
{%- endfor %}
{%- else if method.checks_status %}
    {{ method.c_result_type }} status = {{ method.c_function }}({{ method.arguments }});
{%- for parameter in method.byte_arrays %}
    if ({{ parameter.pointer }} != NULL) {
        (*env)->ReleaseByteArrayElements(env, {{ parameter.name }}, {{ parameter.pointer }}, JNI_ABORT);
    }
{%- endfor %}
    boltffi_jni_throw_status(env, status);
{%- else %}
    (void)env;
    {{ method.c_result_type }} result = {{ method.c_function }}({{ method.arguments }});
{%- for parameter in method.byte_arrays %}
    if ({{ parameter.pointer }} != NULL) {
        (*env)->ReleaseByteArrayElements(env, {{ parameter.name }}, {{ parameter.pointer }}, JNI_ABORT);
    }
{%- endfor %}
{%- if method.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, result);
{%- else if method.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));
{%- else %}
    return {{ method.return_value }};
{%- endif %}
{%- endif %}
}
{%- endfor %}

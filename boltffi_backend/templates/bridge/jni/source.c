#include <jni.h>
#include <stdint.h>
#include <stdbool.h>
{%- if uses_limits %}
#include <limits.h>
{%- endif %}
{%- if uses_callback_handles || uses_closure_handles || uses_byte_arrays %}
#include <stdlib.h>
{%- endif %}
#if defined(__ANDROID__)
#include <pthread.h>
#endif

#include {{ c_header }}

{%- if uses_lifecycle %}
{% include "bridge/jni/runtime.c" %}
{%- endif %}

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

{%- if uses_callback_handles %}
{% include "bridge/jni/callback.c" %}
{%- endif %}

{%- if closure_handles.len() > 0 %}
{% include "bridge/jni/closure_callback_handle.c" %}
{%- endif %}

{%- if closures.len() > 0 %}
{% include "bridge/jni/closure_registration.c" %}
{%- endif %}

{%- if callbacks.len() > 0 %}
{% include "bridge/jni/callback_registration.c" %}
{%- endif %}

{%- for invoker in callback_completions %}
{% include "bridge/jni/callback_completion.c" %}
{%- endfor %}

{%- if uses_lifecycle %}
{% include "bridge/jni/lifecycle.c" %}
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

static jbyteArray boltffi_jni_bytes_to_byte_array(JNIEnv *env, const uint8_t *bytes, uintptr_t len) {
    if (bytes == NULL && len != 0) {
        boltffi_jni_throw_runtime(env, "BoltFFI byte slice pointer was null with non-zero length");
        return NULL;
    }
    if (len > (uintptr_t)INT32_MAX) {
        boltffi_jni_throw_runtime(env, "BoltFFI byte slice too large for Java byte array");
        return NULL;
    }
    jbyteArray array = (*env)->NewByteArray(env, (jsize)len);
    if (array == NULL) {
        return NULL;
    }
    if (len != 0) {
        (*env)->SetByteArrayRegion(env, array, 0, (jsize)len, (const jbyte *)bytes);
        if ((*env)->ExceptionCheck(env)) {
            (*env)->DeleteLocalRef(env, array);
            return NULL;
        }
    }
    return array;
}

static FfiBuf_u8 boltffi_jni_byte_array_to_buffer(JNIEnv *env, jbyteArray array) {
    FfiBuf_u8 empty = {0};
    if (array == NULL) {
        boltffi_jni_throw_runtime(env, "BoltFFI byte array return was null");
        return empty;
    }
    jsize len = (*env)->GetArrayLength(env, array);
    if (len == 0) {
        return empty;
    }
    uint8_t *bytes = (uint8_t *)malloc((size_t)len);
    if (bytes == NULL) {
        boltffi_jni_throw_runtime(env, "failed to allocate BoltFFI byte array return");
        return empty;
    }
    (*env)->GetByteArrayRegion(env, array, 0, len, (jbyte *)bytes);
    if ((*env)->ExceptionCheck(env)) {
        free(bytes);
        return empty;
    }
    FfiBuf_u8 buffer = {
        .ptr = bytes,
        .len = (uintptr_t)len,
        .cap = (uintptr_t)len,
        .align = 1,
    };
    return buffer;
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
{% include "bridge/jni/method.c" %}
{%- endfor %}

{%- for batch in direct_stream_batches %}
{% include "bridge/jni/stream_direct_batch.c" %}
{%- endfor %}

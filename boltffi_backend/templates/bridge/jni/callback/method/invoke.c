{%- if method.returns_void %}
    (*env)->CallStaticVoidMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{% include "bridge/jni/callback/method/cleanup.c" %}
    if (boltffi_jni_clear_exception(env)) {
{%- for completion in method.completions %}
        {{ completion.callback }}({{ completion.failure_arguments }});
{%- endfor %}
    }
    boltffi_jni_exit(env, attached);
{%- else %}
{%- if method.returns_byte_array %}
    jbyteArray __boltffi_return_array = (jbyteArray)(*env)->CallStaticObjectMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- else if method.returns_callback_handle %}
    jlong __boltffi_return_handle = (*env)->CallStaticLongMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- else if method.returns_closure %}
    jlong __boltffi_return_handle = (*env)->CallStaticLongMethod(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- else %}
    {{ method.c_return_type }} result = ({{ method.c_return_type }})(*env)->CallStatic{{ method.call_method_suffix }}Method(env, {{ callback.global_class }}, {{ method.method_id }}, {{ method.jni_arguments }});
{%- endif %}
{% include "bridge/jni/callback/method/cleanup.c" %}
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(env, attached);
        return {{ method.failure_value }};
    }
{%- if method.returns_bytes %}
    {{ method.c_return_type }} result = boltffi_jni_byte_array_to_buffer(env, __boltffi_return_array);
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
    if (boltffi_jni_clear_exception(env)) {
        boltffi_jni_exit(env, attached);
        return {{ method.failure_value }};
    }
{%- else if method.returns_record %}
    {{ method.c_return_type }} result = {0};
    if (!boltffi_jni_read_record(env, __boltffi_return_array, (uintptr_t)sizeof(result), &result)) {
        (*env)->DeleteLocalRef(env, __boltffi_return_array);
        boltffi_jni_clear_exception(env);
        boltffi_jni_exit(env, attached);
        return {{ method.failure_value }};
    }
    (*env)->DeleteLocalRef(env, __boltffi_return_array);
{%- else if method.returns_callback_handle %}
    {%- match method.callback_handle_constructor %}
    {%- when Some with (create_handle) %}
    {{ method.c_return_type }} result = {{ create_handle }}((uint64_t)__boltffi_return_handle);
    {%- when None %}
    {{ method.c_return_type }} result = {{ method.failure_value }};
    {%- endmatch %}
{%- else if method.returns_closure %}
    {%- match method.closure_return %}
    {%- when Some with (closure_return) %}
    if ({{ closure_return.output }} == NULL) {
        {{ closure_return.release }}((void *)(uintptr_t)__boltffi_return_handle);
        boltffi_jni_exit(env, attached);
        return {{ method.failure_value }};
    }
    typedef struct {
        {{ closure_return.invoke_field }};
        void *context;
        void (*release)(void *);
    } {{ closure_return.storage }};
    {{ closure_return.storage }} __boltffi_return = {
        .invoke = {{ closure_return.invoke }},
        .context = (void *)(uintptr_t)__boltffi_return_handle,
        .release = {{ closure_return.release }},
    };
    *(({{ closure_return.storage }} *){{ closure_return.output }}) = __boltffi_return;
    {{ method.c_return_type }} result = ({{ method.c_return_type }}){.code = 0};
    {%- when None %}
    {{ method.c_return_type }} result = {{ method.failure_value }};
    {%- endmatch %}
{%- endif %}
    boltffi_jni_exit(env, attached);
    return result;
{%- endif %}

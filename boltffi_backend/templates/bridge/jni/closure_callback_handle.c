{%- for handle in closure_handles %}

typedef struct {
    {{ handle.call_field }};
    void *context;
    void (*release)(void *);
} {{ handle.ty }};

static {{ handle.ty }} *{{ handle.ref_ }}(jlong value) {
    return value == 0 ? NULL : ({{ handle.ty }} *)(uintptr_t)value;
}

static void {{ handle.release }}(jlong value) {
    {{ handle.ty }} *closure = {{ handle.ref_ }}(value);
    if (closure == NULL) {
        return;
    }
    if (closure->release != NULL) {
        closure->release(closure->context);
    }
    free(closure);
}

static jlong {{ handle.new }}(JNIEnv *env, {{ handle.call_field }}, void *context, void (*release)(void *)) {
    {{ handle.ty }} *closure = ({{ handle.ty }} *)malloc(sizeof({{ handle.ty }}));
    if (closure == NULL) {
        if (release != NULL) {
            release(context);
        }
        boltffi_jni_throw_runtime(env, "failed to allocate BoltFFI closure handle");
        return 0;
    }
    closure->call = call;
    closure->context = context;
    closure->release = release;
    return (jlong)(uintptr_t)closure;
}

JNIEXPORT void JNICALL {{ handle.release_symbol }}(JNIEnv *env, jclass cls, jlong value) {
    (void)env;
    (void)cls;
    {{ handle.release }}(value);
}

JNIEXPORT {{ handle.jni_return_type }} JNICALL {{ handle.call_symbol }}(JNIEnv *env, jclass cls, jlong value{% for argument in handle.closure.arguments %}, {{ argument.jni_type }} {{ argument.name }}{% endfor %}) {
    (void)env;
    (void)cls;
    {{ handle.ty }} *closure = {{ handle.ref_ }}(value);
    if (closure == NULL || closure->call == NULL) {
{%- if handle.closure.returns_void %}
        return;
{%- else %}
        return {{ handle.failure_value }};
{%- endif %}
    }
{%- if handle.closure.returns_void %}
    closure->call(closure->context{% for argument in handle.closure.arguments %}, ({{ argument.c_type }}){{ argument.name }}{% endfor %});
{%- else %}
    {{ handle.closure.c_return_type }} result = closure->call(closure->context{% for argument in handle.closure.arguments %}, ({{ argument.c_type }}){{ argument.name }}{% endfor %});
{%- if handle.closure.returns_bytes %}
    return boltffi_jni_buffer_to_byte_array(env, result);
{%- else if handle.closure.returns_record %}
    return boltffi_jni_record_to_byte_array(env, &result, (uintptr_t)sizeof(result));
{%- else %}
    return ({{ handle.jni_return_type }})result;
{%- endif %}
{%- endif %}
}
{%- endfor %}

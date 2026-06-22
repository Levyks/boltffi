static void {{ callback.unload }}(JNIEnv *env) {
    if ({{ callback.global_class }} != NULL) {
        (*env)->DeleteGlobalRef(env, {{ callback.global_class }});
    }
    {{ callback.global_class }} = NULL;
    {{ callback.free_method }} = NULL;
    {{ callback.clone_method }} = NULL;
{%- for method in callback.methods %}
    {{ method.method_id }} = NULL;
{%- endfor %}
}

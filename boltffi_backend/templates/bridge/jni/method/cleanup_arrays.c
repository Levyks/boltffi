{%- for cleanup in method.borrowed_arrays %}
        if ({{ cleanup.pointer }} != NULL) {
            (*env)->{{ cleanup.releaser }}(env, {{ cleanup.name }}, {{ cleanup.pointer }}, JNI_ABORT);
        }
{%- endfor %}

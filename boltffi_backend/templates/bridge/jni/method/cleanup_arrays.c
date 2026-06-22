{%- for cleanup in method.borrowed_arrays %}
        if ({{ cleanup.pointer }} != NULL) {
{%- if let Some(stack) = cleanup.stack_copy %}
            if ({{ stack.needs_release }}) {
                (*env)->{{ cleanup.releaser }}(env, {{ cleanup.name }}, {{ cleanup.pointer }}, JNI_ABORT);
            }
{%- else %}
            (*env)->{{ cleanup.releaser }}(env, {{ cleanup.name }}, {{ cleanup.pointer }}, JNI_ABORT);
{%- endif %}
        }
{%- endfor %}

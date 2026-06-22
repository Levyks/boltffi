{%- for bytes in method.byte_arrays %}
        (*env)->DeleteLocalRef(env, {{ bytes.name }});
{%- endfor %}
{%- for vector in method.direct_vectors %}
        if ({{ vector.array }} != NULL) {
            (*env)->DeleteLocalRef(env, {{ vector.array }});
        }
{%- endfor %}
{%- for record in method.record_arrays %}
        (*env)->DeleteLocalRef(env, {{ record.array }});
{%- endfor %}
{%- for handle in method.callback_handles %}
        boltffi_jni_callback_handle_release(boltffi_jni_callback_handle_ref({{ handle.handle }}));
{%- endfor %}
{%- for handle in method.closure_handles %}
        {{ handle.handle_release }}({{ handle.handle }});
{%- endfor %}

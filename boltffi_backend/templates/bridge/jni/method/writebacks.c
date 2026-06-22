{%- for parameter in method.record_arrays %}
{%- if let Some(writeback) = parameter.writeback %}
        (*env)->SetByteArrayRegion(env, {{ parameter.name }}, 0, (jsize)sizeof({{ writeback.c_type }}), (const jbyte *)&{{ writeback.local }});
        if ((*env)->ExceptionCheck(env)) {
{% include "bridge/jni/method/error_return.c" %}
        }
{%- endif %}
{%- endfor %}

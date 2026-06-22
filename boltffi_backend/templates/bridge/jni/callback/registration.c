static jclass {{ callback.global_class }} = NULL;
static jmethodID {{ callback.free_method }} = NULL;
static jmethodID {{ callback.clone_method }} = NULL;
{%- for method in callback.methods %}
static jmethodID {{ method.method_id }} = NULL;
{%- endfor %}

{% include "bridge/jni/callback/free.c" %}

{% include "bridge/jni/callback/clone.c" %}

{%- for method in callback.methods %}
{% include "bridge/jni/callback/method.c" %}
{%- endfor %}

{% include "bridge/jni/callback/vtable.c" %}

{% include "bridge/jni/callback/load.c" %}

{% include "bridge/jni/callback/unload.c" %}

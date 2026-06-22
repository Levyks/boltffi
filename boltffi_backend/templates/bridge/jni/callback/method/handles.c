{%- for callback_handle in method.callback_handles %}
    jlong {{ callback_handle.handle }} = 0;
{%- endfor %}
{%- for closure_handle in method.closure_handles %}
    jlong {{ closure_handle.handle }} = 0;
{%- endfor %}

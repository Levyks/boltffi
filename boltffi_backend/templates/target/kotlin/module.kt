package {{ package }}

{{ runtime }}

@Suppress("FunctionName")
private object Native {
{%- for function in native_functions %}
    @JvmStatic external fun {{ function.name() }}({% for parameter in function.parameters() %}{{ parameter.name() }}: {{ parameter.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}): {{ function.returns() }}
{%- endfor %}
}

{%- for record in records %}

{{ record }}
{%- endfor %}

{%- for enumeration in enumerations %}

{{ enumeration }}
{%- endfor %}

{%- for function in functions %}

{{ function }}
{%- endfor %}

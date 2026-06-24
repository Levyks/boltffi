enum class {{ enumeration.name() }}(val value: {{ enumeration.value_type() }}) {
{%- for variant in enumeration.variants() %}
    {{ variant.name() }}({{ variant.value() }}){% if !loop.last %},{% else %};{% endif %}
{%- endfor %}

    companion object {
        fun fromValue(value: {{ enumeration.value_type() }}): {{ enumeration.name() }} =
            entries.first { it.value == value }
    }
}

{{ enumeration.documentation() }}public enum {{ enumeration.name() }}: {{ enumeration.raw_type() }}, Hashable, Sendable, CaseIterable {
{%- for variant in enumeration.variants() %}
{{ variant.documentation() }}    case {{ variant.name() }} = {{ variant.discriminant() }}
{%- endfor %}

    @usableFromInline init(fromC c: {{ enumeration.raw_type() }}) {
        self = {{ enumeration.name() }}(rawValue: c)!
    }

    @usableFromInline var cValue: {{ enumeration.raw_type() }} {
        rawValue
    }
}

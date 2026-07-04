{{ record.documentation() }}public struct {{ record.name() }}: Hashable, Equatable, Sendable {
{%- for field in record.fields() %}
{{ field.documentation() }}    public var {{ field.name() }}: {{ field.ty() }}
{%- endfor %}

    public init({% for field in record.fields() %}{{ field.name() }}: {{ field.ty() }}{% if !loop.last %}, {% endif %}{% endfor %}) {
{%- for field in record.fields() %}
        {{ field.assignment() }}
{%- endfor %}
    }

    @usableFromInline init(fromC c: {{ record.c_type() }}) {
        self.init({% for field in record.fields() %}{{ field.c_initializer_argument() }}{% if !loop.last %}, {% endif %}{% endfor %})
    }

    @usableFromInline var cValue: {{ record.c_type() }} {
        {{ record.c_type() }}({% for field in record.fields() %}{{ field.c_value_argument() }}{% if !loop.last %}, {% endif %}{% endfor %})
    }
}

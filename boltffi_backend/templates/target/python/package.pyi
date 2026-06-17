from __future__ import annotations

{% if !records.is_empty() %}
from dataclasses import dataclass

{% endif %}
{% if !enums.is_empty() %}
from enum import IntEnum

{% endif %}
MODULE_NAME: str
PACKAGE_NAME: str
PACKAGE_VERSION: str | None
{% for record in records %}
@dataclass(frozen=True, slots=True)
class {{ record.class_name }}:
{%- for field in record.fields %}
    {{ field.name }}: {{ field.annotation }}
{%- endfor %}

{% endfor %}
{% for enumeration in enums %}
class {{ enumeration.class_name }}(IntEnum):
{%- for variant in enumeration.variants %}
    {{ variant.name }} = {{ variant.value }}
{%- endfor %}

{% endfor %}
{% for function in functions %}
def {{ function.python_name }}({% for parameter in function.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ function.return_annotation }}: ...
{%- endfor %}

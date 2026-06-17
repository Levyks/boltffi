from __future__ import annotations

{% if !direct_records.is_empty() %}
from dataclasses import dataclass

{% endif %}
{% if !enums.is_empty() %}
from enum import IntEnum

{% endif %}
MODULE_NAME: str
PACKAGE_NAME: str
PACKAGE_VERSION: str | None
{% for record in direct_records %}
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
{% for class in classes %}
class {{ class.class_name }}:
    _handle: int
{% if !class.init.is_empty() %}
{% for init in class.init %}
    def __init__(self{% for parameter in init.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> None: ...
{% endfor %}
{% else %}
    def __init__(self) -> None: ...
{% endif %}
    @classmethod
    def _from_handle(cls, handle: int) -> "{{ class.class_name }}": ...
    def __del__(self) -> None: ...
{%- for constructor in class.constructors %}
    @classmethod
    def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ class.class_name }}": ...
{%- endfor %}
{%- for method in class.static_methods %}
    @staticmethod
    def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for method in class.instance_methods %}
    def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}: ...
{%- endfor %}
{%- for stream in class.streams %}
    def {{ stream.python_name }}(self) -> "{{ stream.subscription_class }}": ...
{%- endfor %}

{% for stream in class.streams %}
class {{ stream.subscription_class }}:
    _handle: int | None
    def __init__(self) -> None: ...
    @classmethod
    def _from_handle(cls, handle: int) -> "{{ stream.subscription_class }}": ...
    def __del__(self) -> None: ...
    def pop_batch(self, max_count: int = 16) -> list[{{ stream.item_annotation }}]: ...
    def wait(self, timeout_milliseconds: int) -> int: ...
    def unsubscribe(self) -> None: ...

{% endfor %}
{% endfor %}
{% for function in functions %}
def {{ function.python_name }}({% for parameter in function.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ function.return_annotation }}: ...
{%- endfor %}

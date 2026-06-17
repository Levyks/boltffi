from __future__ import annotations

MODULE_NAME: str
PACKAGE_NAME: str
PACKAGE_VERSION: str | None
{% for function in functions %}
def {{ function.python_name }}({% for parameter in function.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ function.return_annotation }}: ...
{%- endfor %}

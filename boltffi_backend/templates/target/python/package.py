from __future__ import annotations

{% if !records.is_empty() %}
from dataclasses import dataclass

{% endif %}
{% if !enums.is_empty() %}
from enum import IntEnum

{% endif %}
import sys
from pathlib import Path

from . import _native


def _shared_library_filename() -> str:
    if sys.platform == "win32":
        return "{{ library_name }}.dll"
    if sys.platform == "darwin":
        return "lib{{ library_name }}.dylib"
    return "lib{{ library_name }}.so"


_native._initialize_loader(str(Path(__file__).resolve().with_name(_shared_library_filename())))

{% for record in records %}
@dataclass(frozen=True, slots=True)
class {{ record.class_name }}:
{%- for field in record.fields %}
    {{ field.name }}: {{ field.annotation }}
{%- endfor %}


_native.{{ record.register_method }}({{ record.class_name }})

{% endfor %}
{% for enumeration in enums %}
class {{ enumeration.class_name }}(IntEnum):
{%- for variant in enumeration.variants %}
    {{ variant.name }} = {{ variant.value }}
{%- endfor %}


_native.{{ enumeration.register_method }}({{ enumeration.class_name }})

{% endfor %}
{% for function in functions %}
{{ function }} = _native.{{ function }}
{%- endfor %}

MODULE_NAME = {{ module_name_literal }}
PACKAGE_NAME = {{ package_name_literal }}
PACKAGE_VERSION = {{ package_version_literal }}

__all__ = [
    "MODULE_NAME",
    "PACKAGE_NAME",
    "PACKAGE_VERSION",
{%- for record in records %}
    "{{ record.class_name }}",
{%- endfor %}
{%- for enumeration in enums %}
    "{{ enumeration.class_name }}",
{%- endfor %}
{%- for function in functions %}
    "{{ function }}",
{%- endfor %}
]

from __future__ import annotations

{% if !direct_records.is_empty() %}
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

{% for record in direct_records %}
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
{% for class in classes %}
class {{ class.class_name }}:
    __slots__ = ("_handle",)

{% if !class.init.is_empty() %}
{% for init in class.init %}
    def __init__(self{% for parameter in init.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> None:
        self._handle = _native.{{ init.native_name }}({{ init.arguments }})
{% endfor %}
{% else %}
    def __init__(self) -> None:
        raise TypeError("{{ class.class_name }} cannot be constructed directly")
{% endif %}

    @classmethod
    def _from_handle(cls, handle: int) -> "{{ class.class_name }}":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.{{ class.release_method }}(handle)
{%- for constructor in class.constructors %}

    @classmethod
    def {{ constructor.python_name }}(cls{% for parameter in constructor.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> "{{ class.class_name }}":
        return cls._from_handle(_native.{{ constructor.native_name }}({{ constructor.arguments }}))
{%- endfor %}
{%- for method in class.static_methods %}

    @staticmethod
    def {{ method.python_name }}({% for parameter in method.parameters %}{{ parameter.name }}: {{ parameter.annotation }}{% if !loop.last %}, {% endif %}{% endfor %}) -> {{ method.return_annotation }}:
{%- if method.wraps_return_handle %}
        return {{ method.return_class }}._from_handle(_native.{{ method.native_name }}({{ method.arguments }}))
{%- elif method.returns_value %}
        return _native.{{ method.native_name }}({{ method.arguments }})
{%- else %}
        _native.{{ method.native_name }}({{ method.arguments }})
{%- endif %}
{%- endfor %}
{%- for method in class.instance_methods %}

    def {{ method.python_name }}(self{% for parameter in method.parameters %}, {{ parameter.name }}: {{ parameter.annotation }}{% endfor %}) -> {{ method.return_annotation }}:
{%- if method.wraps_return_handle %}
        return {{ method.return_class }}._from_handle(_native.{{ method.native_name }}({{ method.arguments }}))
{%- elif method.returns_value %}
        return _native.{{ method.native_name }}({{ method.arguments }})
{%- else %}
        _native.{{ method.native_name }}({{ method.arguments }})
{%- endif %}
{%- endfor %}
{%- for stream in class.streams %}

    def {{ stream.python_name }}(self) -> "{{ stream.subscription_class }}":
        return {{ stream.subscription_class }}._from_handle(_native.{{ stream.subscribe_method }}(self._handle))
{%- endfor %}

{% for stream in class.streams %}
class {{ stream.subscription_class }}:
    __slots__ = ("_handle",)

    def __init__(self) -> None:
        raise TypeError("{{ stream.subscription_class }} cannot be constructed directly")

    @classmethod
    def _from_handle(cls, handle: int) -> "{{ stream.subscription_class }}":
        value = cls.__new__(cls)
        value._handle = handle
        return value

    def __del__(self) -> None:
        handle = getattr(self, "_handle", None)
        if handle is not None:
            self._handle = None
            _native.{{ stream.free_method }}(handle)

    def pop_batch(self, max_count: int = 16) -> list[{{ stream.item_annotation }}]:
        return _native.{{ stream.pop_batch_method }}(self._require_handle(), max_count)

    def wait(self, timeout_milliseconds: int) -> int:
        return _native.{{ stream.wait_method }}(self._require_handle(), timeout_milliseconds)

    def unsubscribe(self) -> None:
        handle = self._require_handle()
        self._handle = None
        _native.{{ stream.unsubscribe_method }}(handle)
        _native.{{ stream.free_method }}(handle)

    def _require_handle(self) -> int:
        handle = self._handle
        if handle is None:
            raise RuntimeError("stream subscription is closed")
        return handle

{% endfor %}
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
{%- for record in direct_records %}
    "{{ record.class_name }}",
{%- endfor %}
{%- for enumeration in enums %}
    "{{ enumeration.class_name }}",
{%- endfor %}
{%- for class in classes %}
    "{{ class.class_name }}",
{%- for stream in class.streams %}
    "{{ stream.subscription_class }}",
{%- endfor %}
{%- endfor %}
{%- for function in functions %}
    "{{ function }}",
{%- endfor %}
]

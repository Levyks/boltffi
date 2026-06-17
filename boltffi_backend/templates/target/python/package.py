from __future__ import annotations

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
{%- for function in functions %}
    "{{ function }}",
{%- endfor %}
]


{% if support.uses_wire_arguments() %}
static void boltffi_python_write_u32_le(uint8_t *buffer, uint32_t value) {
    buffer[0] = (uint8_t)(value & 0xffu);
    buffer[1] = (uint8_t)((value >> 8) & 0xffu);
    buffer[2] = (uint8_t)((value >> 16) & 0xffu);
    buffer[3] = (uint8_t)((value >> 24) & 0xffu);
}

static int boltffi_python_wire_payload(const uint8_t *payload, Py_ssize_t payload_len, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    if (payload_len < 0 || (uint64_t)payload_len > UINT32_MAX || payload_len > PY_SSIZE_T_MAX - 4) {
        PyErr_SetString(PyExc_OverflowError, "payload is too large");
        return 0;
    }
    Py_ssize_t wire_len = payload_len + 4;
    PyObject *wire = PyBytes_FromStringAndSize(NULL, wire_len);
    if (wire == NULL) {
        return 0;
    }
    uint8_t *bytes = (uint8_t *)PyBytes_AS_STRING(wire);
    boltffi_python_write_u32_le(bytes, (uint32_t)payload_len);
    if (payload_len > 0) {
        memcpy(bytes + 4, payload, (size_t)payload_len);
    }
    *out_wire = wire;
    *out_ptr = bytes;
    *out_len = (uintptr_t)wire_len;
    return 1;
}
{% endif %}
{% if support.uses_wire_strings() %}
static int boltffi_python_wire_string(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    Py_ssize_t len = 0;
    const char *utf8 = PyUnicode_AsUTF8AndSize(value, &len);
    if (utf8 == NULL) {
        return 0;
    }
    return boltffi_python_wire_payload((const uint8_t *)utf8, len, out_wire, out_ptr, out_len);
}
{% endif %}
{% if support.uses_wire_bytes() %}
static int boltffi_python_wire_bytes(PyObject *value, PyObject **out_wire, const uint8_t **out_ptr, uintptr_t *out_len) {
    Py_buffer view;
    if (PyObject_GetBuffer(value, &view, PyBUF_CONTIG_RO) < 0) {
        return 0;
    }
    int ok = boltffi_python_wire_payload((const uint8_t *)view.buf, view.len, out_wire, out_ptr, out_len);
    PyBuffer_Release(&view);
    return ok;
}
{% endif %}
{% if support.uses_owned_buffers() %}
static uint32_t boltffi_python_read_u32_le(const uint8_t *buffer) {
    return ((uint32_t)buffer[0])
        | ((uint32_t)buffer[1] << 8)
        | ((uint32_t)buffer[2] << 16)
        | ((uint32_t)buffer[3] << 24);
}

static int boltffi_python_validate_owned_buffer(FfiBuf_u8 buffer) {
    if (buffer.ptr == NULL && buffer.len != 0) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned invalid buffer");
        return 0;
    }
    if (buffer.len < 4) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated wire buffer");
        return 0;
    }
    if (buffer.len > PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native buffer is too large");
        return 0;
    }
    return 1;
}

static void boltffi_python_release_owned_buffer(FfiBuf_u8 buffer) {
    {{ support.free_buffer() }}(buffer);
}
{% endif %}
{% if support.uses_owned_utf8() %}
static PyObject *boltffi_python_decode_owned_utf8(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_buffer(buffer)) {
        goto done;
    }
    uint32_t len = boltffi_python_read_u32_le(buffer.ptr);
    if ((uintptr_t)len > buffer.len - 4) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated string buffer");
        goto done;
    }
    if (len > (uint32_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native string is too large");
        goto done;
    }
    result = PyUnicode_FromStringAndSize((const char *)(buffer.ptr + 4), (Py_ssize_t)len);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% if support.uses_owned_bytes() %}
static PyObject *boltffi_python_decode_owned_bytes(FfiBuf_u8 buffer) {
    PyObject *result = NULL;
    if (!boltffi_python_validate_owned_buffer(buffer)) {
        goto done;
    }
    uint32_t len = boltffi_python_read_u32_le(buffer.ptr);
    if ((uintptr_t)len > buffer.len - 4) {
        PyErr_SetString(PyExc_RuntimeError, "native function returned truncated bytes buffer");
        goto done;
    }
    if (len > (uint32_t)PY_SSIZE_T_MAX) {
        PyErr_SetString(PyExc_OverflowError, "native bytes are too large");
        goto done;
    }
    result = PyBytes_FromStringAndSize((const char *)(buffer.ptr + 4), (Py_ssize_t)len);
done:
    boltffi_python_release_owned_buffer(buffer);
    return result;
}
{% endif %}
{% for primitive in support.primitives() %}
{% if primitive.is_bool() %}
static int {{ primitive.parser }}(PyObject *value, bool *out) {
    if (!PyBool_Check(value)) {
        PyErr_SetString(PyExc_TypeError, "expected bool");
        return 0;
    }
    *out = value == Py_True;
    return 1;
}

static PyObject *{{ primitive.boxer }}(bool value) {
    return PyBool_FromLong(value ? 1 : 0);
}
{% endif %}
{% if primitive.is_i8() %}
static int {{ primitive.parser }}(PyObject *value, int8_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < INT8_MIN || parsed > INT8_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected i8");
        return 0;
    }
    *out = (int8_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int8_t value) {
    return PyLong_FromLong((long)value);
}
{% endif %}
{% if primitive.is_u8() %}
static int {{ primitive.parser }}(PyObject *value, uint8_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed > UINT8_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected u8");
        return 0;
    }
    *out = (uint8_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint8_t value) {
    return PyLong_FromUnsignedLong((unsigned long)value);
}
{% endif %}
{% if primitive.is_i16() %}
static int {{ primitive.parser }}(PyObject *value, int16_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < INT16_MIN || parsed > INT16_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected i16");
        return 0;
    }
    *out = (int16_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int16_t value) {
    return PyLong_FromLong((long)value);
}
{% endif %}
{% if primitive.is_u16() %}
static int {{ primitive.parser }}(PyObject *value, uint16_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed > UINT16_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected u16");
        return 0;
    }
    *out = (uint16_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint16_t value) {
    return PyLong_FromUnsignedLong((unsigned long)value);
}
{% endif %}
{% if primitive.is_i32() %}
static int {{ primitive.parser }}(PyObject *value, int32_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < INT32_MIN || parsed > INT32_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected i32");
        return 0;
    }
    *out = (int32_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int32_t value) {
    return PyLong_FromLong((long)value);
}
{% endif %}
{% if primitive.is_u32() %}
static int {{ primitive.parser }}(PyObject *value, uint32_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed > UINT32_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected u32");
        return 0;
    }
    *out = (uint32_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint32_t value) {
    return PyLong_FromUnsignedLong((unsigned long)value);
}
{% endif %}
{% if primitive.is_i64() %}
static int {{ primitive.parser }}(PyObject *value, int64_t *out) {
    long long parsed = PyLong_AsLongLong(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (int64_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(int64_t value) {
    return PyLong_FromLongLong((long long)value);
}
{% endif %}
{% if primitive.is_u64() %}
static int {{ primitive.parser }}(PyObject *value, uint64_t *out) {
    unsigned long long parsed = PyLong_AsUnsignedLongLong(value);
    if (parsed == (unsigned long long)-1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (uint64_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uint64_t value) {
    return PyLong_FromUnsignedLongLong((unsigned long long)value);
}
{% endif %}
{% if primitive.is_isize() %}
static int {{ primitive.parser }}(PyObject *value, intptr_t *out) {
    Py_ssize_t parsed = PyLong_AsSsize_t(value);
    if (parsed == -1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (intptr_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(intptr_t value) {
    return PyLong_FromSsize_t((Py_ssize_t)value);
}
{% endif %}
{% if primitive.is_usize() %}
static int {{ primitive.parser }}(PyObject *value, uintptr_t *out) {
    size_t parsed = PyLong_AsSize_t(value);
    if (parsed == (size_t)-1 && PyErr_Occurred()) {
        return 0;
    }
    *out = (uintptr_t)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(uintptr_t value) {
    return PyLong_FromSize_t((size_t)value);
}
{% endif %}
{% if primitive.is_f32() %}
static int {{ primitive.parser }}(PyObject *value, float *out) {
    double parsed = PyFloat_AsDouble(value);
    if (parsed == -1.0 && PyErr_Occurred()) {
        return 0;
    }
    if (parsed < -FLT_MAX || parsed > FLT_MAX) {
        PyErr_SetString(PyExc_OverflowError, "expected f32");
        return 0;
    }
    *out = (float)parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(float value) {
    return PyFloat_FromDouble((double)value);
}
{% endif %}
{% if primitive.is_f64() %}
static int {{ primitive.parser }}(PyObject *value, double *out) {
    double parsed = PyFloat_AsDouble(value);
    if (parsed == -1.0 && PyErr_Occurred()) {
        return 0;
    }
    *out = parsed;
    return 1;
}

static PyObject *{{ primitive.boxer }}(double value) {
    return PyFloat_FromDouble(value);
}
{% endif %}
{% endfor %}
{% for function in functions %}
{{ function }}
{% endfor %}
static PyMethodDef {{ method_table }}[] = {
{%- for method in methods %}
    {"{{ method.python_name }}", (PyCFunction){{ method.c_function }}, {{ method.flags }}, NULL},
{%- endfor %}
    {NULL, NULL, 0, NULL}
};

static struct PyModuleDef {{ module_definition }} = {
    PyModuleDef_HEAD_INIT,
    "{{ module_name }}",
    NULL,
    -1,
    {{ method_table }},
    NULL,
    NULL,
    NULL,
    {{ free_function }}
};

PyMODINIT_FUNC {{ init_function }}(void) {
    return PyModule_Create(&{{ module_definition }});
}

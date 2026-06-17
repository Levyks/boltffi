
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
{% if support.uses_registered_types() %}
static int boltffi_python_validate_registered_type_object(PyObject *type_object, const char *type_name) {
    if (!PyType_Check(type_object)) {
        PyErr_Format(PyExc_TypeError, "expected type for %s", type_name);
        return 0;
    }
    return 1;
}

static int boltffi_python_store_registered_type(PyObject **type_slot, PyObject *type_object, const char *type_name) {
    if (!boltffi_python_validate_registered_type_object(type_object, type_name)) {
        return 0;
    }
    Py_INCREF(type_object);
    Py_XDECREF(*type_slot);
    *type_slot = type_object;
    return 1;
}

static int boltffi_python_expect_registered_type(PyObject *type_object, const char *type_name) {
    if (type_object != NULL) {
        return 1;
    }
    PyErr_Format(PyExc_ImportError, "native type %s is not registered", type_name);
    return 0;
}

static int boltffi_python_expect_type_instance(PyObject *value, PyObject *type_object, const char *type_name) {
    int is_instance = 0;
    if (!boltffi_python_expect_registered_type(type_object, type_name)) {
        return 0;
    }
    is_instance = PyObject_IsInstance(value, type_object);
    if (is_instance < 0) {
        return 0;
    }
    if (is_instance == 0) {
        PyErr_Format(PyExc_TypeError, "expected %s", type_name);
        return 0;
    }
    return 1;
}

static PyObject *boltffi_python_get_record_field(PyObject *value, const char *record_name, const char *field_name) {
    PyObject *field_value = PyObject_GetAttrString(value, field_name);
    if (field_value == NULL && PyErr_ExceptionMatches(PyExc_AttributeError)) {
        PyErr_Clear();
        PyErr_Format(PyExc_TypeError, "%s is missing field %s", record_name, field_name);
    }
    return field_value;
}

static PyObject *boltffi_python_box_registered_record(PyObject *type_object, PyObject *constructor_args, const char *record_name) {
    PyObject *record_value = NULL;
    if (constructor_args == NULL) {
        return NULL;
    }
    if (!boltffi_python_expect_registered_type(type_object, record_name)) {
        Py_DECREF(constructor_args);
        return NULL;
    }
    record_value = PyObject_CallObject(type_object, constructor_args);
    Py_DECREF(constructor_args);
    return record_value;
}
{% endif %}
{% if support.uses_c_style_enums() %}
typedef PyObject *(*boltffi_python_load_c_style_enum_member_fn)(PyObject *, Py_ssize_t);

typedef struct boltffi_python_c_style_enum_registration {
    PyObject *type_object;
    Py_ssize_t member_count;
    PyObject **members_by_wire_tag;
} boltffi_python_c_style_enum_registration;

static void boltffi_python_release_registered_enum_members(PyObject **members_by_wire_tag, Py_ssize_t member_count) {
    Py_ssize_t member_index = 0;
    for (member_index = 0; member_index < member_count; member_index += 1) {
        Py_XDECREF(members_by_wire_tag[member_index]);
        members_by_wire_tag[member_index] = NULL;
    }
}

static void boltffi_python_clear_c_style_enum_registration(
    boltffi_python_c_style_enum_registration *registration
) {
    Py_XDECREF(registration->type_object);
    registration->type_object = NULL;
    boltffi_python_release_registered_enum_members(
        registration->members_by_wire_tag,
        registration->member_count
    );
}

static PyObject *boltffi_python_load_c_style_enum_member(
    PyObject *type_object,
    const char *enum_name,
    const char *member_name,
    PyObject *native_value
) {
    PyObject *named_member = NULL;
    PyObject *resolved_member = NULL;
    if (native_value == NULL) {
        return NULL;
    }
    named_member = PyObject_GetAttrString(type_object, member_name);
    if (named_member == NULL) {
        return NULL;
    }
    resolved_member = PyObject_CallOneArg(type_object, native_value);
    if (resolved_member == NULL) {
        Py_DECREF(named_member);
        return NULL;
    }
    if (named_member != resolved_member) {
        PyErr_Format(PyExc_ValueError, "native enum %s member %s has the wrong value", enum_name, member_name);
        Py_DECREF(named_member);
        Py_DECREF(resolved_member);
        return NULL;
    }
    Py_DECREF(resolved_member);
    return named_member;
}

static int boltffi_python_store_c_style_enum_registration(
    boltffi_python_c_style_enum_registration *registration,
    PyObject *type_object,
    const char *enum_name,
    boltffi_python_load_c_style_enum_member_fn load_member
) {
    PyObject **loaded_members = NULL;
    Py_ssize_t member_index = 0;
    if (!boltffi_python_validate_registered_type_object(type_object, enum_name)) {
        return 0;
    }
    loaded_members = PyMem_Calloc((size_t)registration->member_count, sizeof(PyObject *));
    if (loaded_members == NULL) {
        PyErr_NoMemory();
        return 0;
    }
    for (member_index = 0; member_index < registration->member_count; member_index += 1) {
        loaded_members[member_index] = load_member(type_object, member_index);
        if (loaded_members[member_index] == NULL) {
            boltffi_python_release_registered_enum_members(loaded_members, registration->member_count);
            PyMem_Free(loaded_members);
            return 0;
        }
    }
    boltffi_python_clear_c_style_enum_registration(registration);
    if (!boltffi_python_store_registered_type(&registration->type_object, type_object, enum_name)) {
        boltffi_python_release_registered_enum_members(loaded_members, registration->member_count);
        PyMem_Free(loaded_members);
        return 0;
    }
    for (member_index = 0; member_index < registration->member_count; member_index += 1) {
        registration->members_by_wire_tag[member_index] = loaded_members[member_index];
    }
    PyMem_Free(loaded_members);
    return 1;
}

static int boltffi_python_expect_enum_instance(
    PyObject *value,
    const boltffi_python_c_style_enum_registration *registration,
    const char *enum_name
) {
    return boltffi_python_expect_type_instance(value, registration->type_object, enum_name);
}

static PyObject *boltffi_python_box_registered_enum_member(
    const boltffi_python_c_style_enum_registration *registration,
    Py_ssize_t member_index,
    const char *enum_name
) {
    PyObject *member = NULL;
    if (!boltffi_python_expect_registered_type(registration->type_object, enum_name)) {
        return NULL;
    }
    if (member_index < 0 || member_index >= registration->member_count) {
        PyErr_SetString(PyExc_RuntimeError, "native enum member index is invalid");
        return NULL;
    }
    if (registration->members_by_wire_tag[member_index] == NULL) {
        PyErr_Format(PyExc_ImportError, "native enum %s member cache is not initialized", enum_name);
        return NULL;
    }
    member = registration->members_by_wire_tag[member_index];
    Py_INCREF(member);
    return member;
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
{% for record in direct_records %}
{{ record }}
{% endfor %}
{% for enumeration in enums %}
{{ enumeration }}
{% endfor %}
{% for class in classes %}
{{ class }}
{% endfor %}
{% for callback in callbacks %}
{{ callback }}
{% endfor %}
static int boltffi_python_bind_host_state(void) {
{%- for binding in host_bindings %}
    if (!{{ binding }}()) {
        return 0;
    }
{%- endfor %}
    return 1;
}

static void boltffi_python_release_host_state(void) {
{%- for cleanup in cleanup %}
    {{ cleanup }};
{%- endfor %}
}
{% for function in functions %}
{{ function }}
{% endfor %}
{% for stream in streams %}
{{ stream }}
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

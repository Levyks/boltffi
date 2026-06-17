static PyObject *{{ members_by_wire_tag }}[{{ variants.len() }}] = {NULL};
static const char *{{ member_names }}[{{ variants.len() }}] = {
{%- for variant in variants %}
    "{{ variant.member_name }}"{% if !loop.last %},{% endif %}
{%- endfor %}
};
static const {{ c_type }} {{ member_native_values }}[{{ variants.len() }}] = {
{%- for variant in variants %}
    {{ variant.native_value }}{% if !loop.last %},{% endif %}
{%- endfor %}
};
static boltffi_python_c_style_enum_registration {{ registration }} = {
    NULL,
    {{ variants.len() }},
    {{ members_by_wire_tag }},
};

static PyObject *{{ load_member }}(PyObject *type_object, Py_ssize_t member_index) {
    PyObject *native_value = NULL;
    PyObject *member = NULL;
    native_value = {{ repr_boxer }}({{ member_native_values }}[member_index]);
    member = boltffi_python_load_c_style_enum_member(
        type_object,
        "{{ class_name }}",
        {{ member_names }}[member_index],
        native_value
    );
    Py_XDECREF(native_value);
    return member;
}

static PyObject *{{ register_wrapper }}(PyObject *self, PyObject *const *args, Py_ssize_t nargs) {
    (void)self;
    if (nargs != 1) {
        PyErr_Format(PyExc_TypeError, "{{ register_method }}() takes 1 positional argument but %zd were given", nargs);
        return NULL;
    }
    if (!boltffi_python_store_c_style_enum_registration(
        &{{ registration }},
        args[0],
        "{{ class_name }}",
        {{ load_member }}
    )) {
        return NULL;
    }
    Py_RETURN_NONE;
}

static int {{ parser }}(PyObject *value, {{ c_type }} *out) {
    if (!boltffi_python_expect_enum_instance(value, &{{ registration }}, "{{ class_name }}")) {
        return 0;
    }
    return {{ repr_parser }}(value, out);
}

static int {{ native_to_wire_tag }}({{ c_type }} value, int32_t *out) {
    switch (value) {
{%- for variant in variants %}
        case {{ variant.native_value }}:
            *out = {{ variant.wire_tag }};
            return 1;
{%- endfor %}
        default:
            PyErr_SetString(PyExc_ValueError, "invalid {{ class_name }} value");
            return 0;
    }
}

static PyObject *{{ box_from_wire_tag }}(int32_t wire_tag) {
    switch (wire_tag) {
{%- for variant in variants %}
        case {{ variant.wire_tag }}:
            return boltffi_python_box_registered_enum_member(
                &{{ registration }},
                {{ variant.member_index }},
                "{{ class_name }}"
            );
{%- endfor %}
        default:
            PyErr_SetString(PyExc_RuntimeError, "native enum wire tag is invalid");
            return NULL;
    }
}

static PyObject *{{ boxer }}({{ c_type }} value) {
    int32_t wire_tag = 0;
    if (!{{ native_to_wire_tag }}(value, &wire_tag)) {
        return NULL;
    }
    return {{ box_from_wire_tag }}(wire_tag);
}

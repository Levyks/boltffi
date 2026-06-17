static void {{ free }}(uint64_t handle) {
    PyGILState_STATE gil = PyGILState_Ensure();
    Py_XDECREF((PyObject *)(uintptr_t)handle);
    PyGILState_Release(gil);
}

static uint64_t {{ clone }}(uint64_t handle) {
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *value = (PyObject *)(uintptr_t)handle;
    Py_XINCREF(value);
    PyGILState_Release(gil);
    return handle;
}

{%- for method in methods %}
static {{ method.returns.c_type }} {{ method.function }}(uint64_t handle{% for param in method.params %}, {{ param.declaration }}{% endfor %}) {
{%- for param in method.params %}
    PyObject *{{ param.object }} = NULL;
{%- endfor %}
    PyObject *callback = NULL;
    PyObject *arguments = NULL;
    PyObject *result = NULL;
{%- if method.returns.has_value() %}
    {{ method.returns.c_type }} {{ method.returns.value }} = {{ method.returns.default_value }};
{%- endif %}
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *receiver = (PyObject *)(uintptr_t)handle;
    callback = PyObject_GetAttrString(receiver, "{{ method.python_name }}");
    if (callback == NULL) {
        goto done;
    }
    arguments = PyTuple_New({{ method.params.len() }});
    if (arguments == NULL) {
        goto done;
    }
{%- for param in method.params %}
    {{ param.object }} = {{ param.boxer }}({{ param.name }});
    if ({{ param.object }} == NULL) {
        goto done;
    }
    PyTuple_SET_ITEM(arguments, {{ loop.index0 }}, {{ param.object }});
    {{ param.object }} = NULL;
{%- endfor %}
    result = PyObject_CallObject(callback, arguments);
    if (result == NULL) {
        goto done;
    }
{%- if method.returns.has_value() %}
    if (!{{ method.returns.parser }}(result, &{{ method.returns.value }})) {
        goto done;
    }
{%- endif %}
done:
    if (PyErr_Occurred()) {
        PyErr_Print();
    }
{%- for param in method.params %}
    Py_XDECREF({{ param.object }});
{%- endfor %}
    Py_XDECREF(result);
    Py_XDECREF(arguments);
    Py_XDECREF(callback);
    PyGILState_Release(gil);
{%- if method.returns.has_value() %}
    return {{ method.returns.value }};
{%- endif %}
}

{%- endfor %}
static {{ vtable_type }} {{ vtable }} = {
{%- for slot in slots %}
    .{{ slot.name }} = {{ slot.function }},
{%- endfor %}
};

static int {{ register }}(void) {
    if ({{ register_storage }} == NULL) {
        PyErr_SetString(PyExc_ImportError, "native library is not initialized");
        return 0;
    }
    {{ register_storage }}(&{{ vtable }});
    return 1;
}

static int {{ parser }}(PyObject *value, BoltFFICallbackHandle *out) {
    if (value == Py_None) {
        PyErr_SetString(PyExc_TypeError, "callback cannot be None");
        return 0;
    }
    Py_INCREF(value);
    *out = {{ create_handle_storage }}((uint64_t)(uintptr_t)value);
    if (out->vtable == NULL) {
        Py_DECREF(value);
        PyErr_SetString(PyExc_RuntimeError, "failed to create callback handle");
        return 0;
    }
    return 1;
}

static int {{ optional_parser }}(PyObject *value, BoltFFICallbackHandle *out) {
    if (value == Py_None) {
        out->handle = 0;
        out->vtable = NULL;
        return 1;
    }
    return {{ parser }}(value, out);
}

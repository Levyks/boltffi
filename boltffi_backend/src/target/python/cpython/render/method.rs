use crate::bridge::python_cext::ExtensionMethod;

pub struct Entry {
    pub python_name: String,
    pub c_function: String,
    pub flags: &'static str,
}

impl Entry {
    pub fn from_method(method: &ExtensionMethod) -> Self {
        Self {
            python_name: method.python_name().to_owned(),
            c_function: method.c_function().to_owned(),
            flags: method.flags().as_c_macro(),
        }
    }
}

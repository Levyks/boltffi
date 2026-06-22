use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::JniType,
};

/// Direct-vector array argument passed from Rust into a JVM callback method.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackDirectVectorArgument<'argument> {
    array: &'argument Identifier,
    pointer: &'argument Identifier,
    length: &'argument Identifier,
    jni_type: JniType,
}

impl<'argument> CallbackDirectVectorArgument<'argument> {
    pub(in crate::bridge::jni::contract::callback) fn new(
        array: &'argument Identifier,
        pointer: &'argument Identifier,
        length: &'argument Identifier,
        jni_type: JniType,
    ) -> Self {
        Self {
            array,
            pointer,
            length,
            jni_type,
        }
    }

    /// Returns the local JNI array variable.
    pub fn array(&self) -> &Identifier {
        self.array
    }

    /// Returns the C vector pointer parameter.
    pub fn pointer(&self) -> &Identifier {
        self.pointer
    }

    /// Returns the C vector length parameter.
    pub fn length(&self) -> &Identifier {
        self.length
    }

    /// Returns the JNI array type.
    pub fn array_type(&self) -> TypeFragment {
        self.jni_type.as_array_type_fragment()
    }

    /// Returns the JNI array element type.
    pub fn element_type(&self) -> TypeFragment {
        self.jni_type.as_type_fragment()
    }

    /// Returns the `New*Array` JNI function table member.
    pub fn new_array(&self) -> &'static str {
        self.jni_type.new_array()
    }

    /// Returns the `Set*ArrayRegion` JNI function table member.
    pub fn set_region(&self) -> &'static str {
        self.jni_type.set_array_region()
    }
}

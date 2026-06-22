use crate::bridge::{
    c::{Identifier, TypeFragment},
    jni::{BytesParameter, DirectVectorParameter},
};

#[derive(Clone)]
pub struct BorrowedArrayParameterView {
    pub name: Identifier,
    pub pointer: Identifier,
    pub length: Identifier,
    pub element_type: TypeFragment,
    pub getter: &'static str,
    pub releaser: &'static str,
}

impl BorrowedArrayParameterView {
    pub fn from_bytes(parameter: &BytesParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            pointer: parameter.pointer().clone(),
            length: parameter.length().clone(),
            element_type: TypeFragment::new("jbyte"),
            getter: "GetByteArrayElements",
            releaser: "ReleaseByteArrayElements",
        }
    }

    pub fn from_direct_vector(parameter: &DirectVectorParameter) -> Self {
        Self {
            name: parameter.name().clone(),
            pointer: parameter.pointer().clone(),
            length: parameter.length().clone(),
            element_type: parameter.element_type(),
            getter: parameter.getter(),
            releaser: parameter.releaser(),
        }
    }
}

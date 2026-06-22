//! Direct-vector parameters for JNI native methods.
//!
//! Direct vectors are Java primitive arrays whose contents can be passed to the
//! C bridge as pointer plus length. The contract records the JNI array type,
//! local pinned pointer, byte length, and C pointer cast needed by the generated
//! method body.

use crate::{
    bridge::{
        c::{self, Expression, Identifier, TypeFragment},
        jni::JniType,
    },
    core::Result,
};

/// Direct-vector JNI parameter expanded to pointer and length C bridge arguments.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct DirectVectorParameter {
    name: Identifier,
    pointer: Identifier,
    length: Identifier,
    pointer_type: TypeFragment,
    jni_type: JniType,
}

impl DirectVectorParameter {
    /// Returns the generated JNI array parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the local pointer variable passed to the C bridge.
    pub fn pointer(&self) -> &Identifier {
        &self.pointer
    }

    /// Returns the local length variable passed to the C bridge.
    pub fn length(&self) -> &Identifier {
        &self.length
    }

    /// Returns the C pointer type expected by the C bridge.
    pub fn pointer_type(&self) -> &TypeFragment {
        &self.pointer_type
    }

    /// Returns the JNI array type.
    pub fn array_type(&self) -> TypeFragment {
        self.jni_type.as_array_type_fragment()
    }

    /// Returns the JNI array element type.
    pub fn element_type(&self) -> TypeFragment {
        self.jni_type.as_type_fragment()
    }

    /// Returns the `Get*ArrayElements` JNI function table member.
    pub fn getter(&self) -> &'static str {
        self.jni_type.array_elements_getter()
    }

    /// Returns the `Release*ArrayElements` JNI function table member.
    pub fn releaser(&self) -> &'static str {
        self.jni_type.array_elements_releaser()
    }

    /// Returns C bridge call arguments produced from this Java array.
    pub fn c_arguments(&self) -> Vec<Expression> {
        vec![
            Expression::cast(
                self.pointer_type.clone(),
                Expression::identifier(self.pointer.clone()),
            ),
            Expression::cast(
                TypeFragment::new("uintptr_t"),
                Expression::identifier(self.length.clone()),
            ),
        ]
    }

    /// Creates a direct-vector JNI parameter from a C direct-vector parameter group.
    pub fn from_c_group(vector: &c::DirectVectorParameter, function: &c::Function) -> Result<Self> {
        let pointer = function.parameter(vector.pointer());
        Ok(Self {
            pointer: Identifier::parse(format!("__boltffi_{}_ptr", vector.name()))?,
            length: Identifier::parse(format!("__boltffi_{}_len", vector.name()))?,
            pointer_type: TypeFragment::anonymous(pointer.ty())?,
            jni_type: JniType::from_direct_vector_element(vector.element())?,
            name: Identifier::escape(vector.name())?,
        })
    }
}

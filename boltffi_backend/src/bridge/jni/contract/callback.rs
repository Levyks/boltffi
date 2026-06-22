use crate::{
    bridge::c::{self, ArgumentList, Expression, Identifier, TypeFragment},
    core::{Error, Result},
};

use boltffi_binding::CallbackId;

const JNI_BRIDGE: &str = "jni";

/// JNI callback handle parameter mapped through a C callback constructor.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackParameter {
    name: Identifier,
    create_handle: Identifier,
}

/// JNI callback handle returned as an owned JVM token.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CallbackReturn {
    callback: CallbackId,
}

impl CallbackParameter {
    /// Returns the generated JNI callback-handle parameter name.
    pub fn name(&self) -> &Identifier {
        &self.name
    }

    /// Returns the JNI parameter type.
    pub fn ty(&self) -> TypeFragment {
        TypeFragment::new("jlong")
    }

    /// Returns the expression passed to the C bridge function.
    pub fn c_argument(&self) -> Expression {
        Expression::call(
            self.create_handle.clone(),
            ArgumentList::from_iter([Expression::cast(
                TypeFragment::new("uint64_t"),
                Expression::identifier(self.name.clone()),
            )]),
        )
    }

    /// Creates a callback parameter from one C callback-handle ABI parameter.
    pub fn from_c_parameter(
        parameter: &c::Parameter,
        callbacks: &[c::Callback],
    ) -> Result<Option<Self>> {
        let c::Type::CallbackHandle(callback) = parameter.ty() else {
            return Ok(None);
        };
        let declaration = callbacks
            .iter()
            .find(|declaration| declaration.id() == *callback)
            .ok_or(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "callback handle parameter has no C callback declaration",
            })?;
        Ok(Some(Self {
            name: Identifier::escape(parameter.name())?,
            create_handle: Identifier::parse(declaration.create_handle().name())?,
        }))
    }
}

impl CallbackReturn {
    /// Returns the JNI method return type.
    pub fn jni_type(&self) -> TypeFragment {
        TypeFragment::new("jlong")
    }

    /// Returns the C result type used by the temporary result variable.
    pub fn c_result_type(&self) -> Result<TypeFragment> {
        TypeFragment::anonymous(&c::Type::CallbackHandle(self.callback))
    }

    /// Returns the expression returned from the JNI method.
    pub fn return_expression(&self, value: Expression) -> Result<Expression> {
        Ok(Expression::call(
            Identifier::parse("boltffi_jni_callback_handle_new_owned")?,
            ArgumentList::from_iter([Expression::identifier(Identifier::parse("env")?), value]),
        ))
    }

    /// Creates a callback return from one C callback-handle ABI type.
    pub fn from_c_type(ty: &c::Type) -> Option<Self> {
        match ty {
            c::Type::CallbackHandle(callback) => Some(Self {
                callback: *callback,
            }),
            _ => None,
        }
    }
}

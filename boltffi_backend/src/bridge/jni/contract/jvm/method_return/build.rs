//! Builder for JVM method return contracts.
//!
//! Callback and closure trampolines call static JVM methods and then translate
//! the result back to the C ABI. This module builds that return contract from
//! the C return type selected by the bridge.

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::{JniType, JvmMethodReturn},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

impl JvmMethodReturn {
    /// Creates a JVM method return contract from one C ABI return type.
    pub fn from_c_type(ty: &c::Type, callbacks: &[c::Callback]) -> Result<Self> {
        match ty {
            c::Type::Void => Ok(Self::Void {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            c::Type::Buffer => Ok(Self::Bytes {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            c::Type::DirectRecord(_) => Ok(Self::Record {
                c_type: TypeFragment::anonymous(ty)?,
            }),
            c::Type::CallbackHandle(callback) => {
                let declaration = callbacks
                    .iter()
                    .find(|declaration| declaration.id() == *callback)
                    .ok_or(Error::BrokenBridgeContract {
                        bridge: JNI_BRIDGE,
                        invariant: "JVM callback handle return has no C callback declaration",
                    })?;
                Ok(Self::CallbackHandle {
                    c_type: TypeFragment::anonymous(ty)?,
                    create_handle: Identifier::parse(declaration.create_handle().name())?,
                })
            }
            ty => Ok(Self::Value {
                c_type: TypeFragment::anonymous(ty)?,
                jni_type: JniType::from_c_type(ty)?,
            }),
        }
    }

    /// Creates the return contract for closure-return callback methods.
    pub fn closure_status() -> Result<Self> {
        Ok(Self::Closure {
            c_type: TypeFragment::anonymous(&c::Type::Status)?,
        })
    }
}

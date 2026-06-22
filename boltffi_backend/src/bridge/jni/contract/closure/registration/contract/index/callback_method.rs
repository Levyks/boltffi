//! Closure registrations referenced by callback methods.
//!
//! Callback vtable slots can receive closure arguments or return closures. This
//! module scans each slot and feeds the shared closure registration index.

use crate::{
    bridge::{c, jni::JvmClassPath},
    core::Result,
};

use super::ClosureRegistrationIndex;

impl ClosureRegistrationIndex {
    pub fn collect_callback_method(
        self,
        class: &JvmClassPath,
        method: &c::CallbackSlot,
        callbacks: &[c::Callback],
    ) -> Result<Self> {
        method.parameter_groups().iter().try_fold(
            self,
            |mut index, group| -> Result<ClosureRegistrationIndex> {
                index.insert_callback_group(class, method, group, callbacks)?;
                Ok(index)
            },
        )
    }

    fn insert_callback_group(
        &mut self,
        class: &JvmClassPath,
        method: &c::CallbackSlot,
        group: &c::ParameterGroup,
        callbacks: &[c::Callback],
    ) -> Result<()> {
        match group {
            c::ParameterGroup::Closure(closure) => {
                self.insert_closure_parameter(
                    class,
                    method.parameter(closure.call()).ty(),
                    closure,
                    true,
                    callbacks,
                )?;
            }
            c::ParameterGroup::ClosureReturn(returned) => {
                self.insert_closure_return(class, returned, callbacks)?;
            }
            _ => {}
        }
        Ok(())
    }
}

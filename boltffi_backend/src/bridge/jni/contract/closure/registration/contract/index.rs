use std::collections::BTreeMap;

use boltffi_binding::ClosureSignature;

use crate::{
    bridge::{c, jni::JvmClassPath},
    core::Result,
};

use super::ClosureRegistration;
use super::build::ClosureRegistrationConstructor;

#[derive(Default)]
pub struct ClosureRegistrationIndex {
    registrations: BTreeMap<ClosureSignature, ClosureRegistration>,
}

impl ClosureRegistrationIndex {
    pub fn from_c_bridge(
        class: &JvmClassPath,
        functions: &[c::Function],
        callbacks: &[c::Callback],
    ) -> Result<Self> {
        functions
            .iter()
            .try_fold(Self::default(), |index, function| {
                index.collect_function(class, function, callbacks)
            })
            .and_then(|index| {
                callbacks
                    .iter()
                    .flat_map(|callback| callback.methods().iter())
                    .try_fold(index, |index, slot| {
                        index.collect_callback_method(class, slot, callbacks)
                    })
            })
    }

    pub fn into_registrations(self) -> Vec<ClosureRegistration> {
        self.registrations.into_values().collect()
    }

    fn collect_function(
        self,
        class: &JvmClassPath,
        function: &c::Function,
        callbacks: &[c::Callback],
    ) -> Result<Self> {
        function.parameter_groups().iter().try_fold(
            self,
            |mut index, group| -> Result<ClosureRegistrationIndex> {
                index.insert_function_group(class, function, group, callbacks)?;
                Ok(index)
            },
        )
    }

    fn collect_callback_method(
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

    fn insert_function_group(
        &mut self,
        class: &JvmClassPath,
        function: &c::Function,
        group: &c::ParameterGroup,
        callbacks: &[c::Callback],
    ) -> Result<()> {
        if let c::ParameterGroup::Closure(closure) = group {
            self.insert_closure_parameter(
                class,
                function.parameter(closure.call()).ty(),
                closure,
                false,
                callbacks,
            )?;
        }
        Ok(())
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

    fn insert_closure_parameter(
        &mut self,
        class: &JvmClassPath,
        call_type: &c::Type,
        closure: &c::ClosureParameter,
        callback_argument: bool,
        callbacks: &[c::Callback],
    ) -> Result<()> {
        let inserted = match self.registrations.get_mut(closure.signature()) {
            Some(registration) => {
                if callback_argument {
                    ClosureRegistrationConstructor::retain_callback_handle(
                        registration,
                        class,
                        call_type,
                    )?;
                }
                false
            }
            None => {
                self.registrations.insert(
                    closure.signature().clone(),
                    ClosureRegistrationConstructor::from_closure_parameter(
                        class,
                        call_type,
                        closure,
                        callback_argument,
                        callbacks,
                    )?,
                );
                true
            }
        };

        if inserted {
            closure
                .parameter_groups()
                .iter()
                .try_for_each(|group| -> Result<()> {
                    if let c::ParameterGroup::Closure(nested) = group {
                        self.insert_closure_parameter(
                            class,
                            closure.parameter(nested.call()).ty(),
                            nested,
                            true,
                            callbacks,
                        )?;
                    }
                    Ok(())
                })?;
        }

        Ok(())
    }

    fn insert_closure_return(
        &mut self,
        class: &JvmClassPath,
        returned: &c::ClosureReturnParameter,
        callbacks: &[c::Callback],
    ) -> Result<()> {
        let inserted = if self.registrations.contains_key(returned.signature()) {
            false
        } else {
            self.registrations.insert(
                returned.signature().clone(),
                ClosureRegistrationConstructor::from_closure_return(class, returned, callbacks)?,
            );
            true
        };

        if inserted {
            returned
                .parameter_groups()
                .iter()
                .try_for_each(|group| -> Result<()> {
                    if let c::ParameterGroup::Closure(nested) = group {
                        self.insert_closure_parameter(
                            class,
                            returned.parameter(nested.call()).ty(),
                            nested,
                            true,
                            callbacks,
                        )?;
                    }
                    Ok(())
                })?;
        }

        Ok(())
    }
}

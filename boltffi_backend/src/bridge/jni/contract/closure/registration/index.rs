use std::collections::{BTreeMap, btree_map::Entry};

use boltffi_binding::ClosureSignature;

use crate::{
    bridge::{
        c,
        jni::{ClosureRegistration, JvmClassPath},
    },
    core::Result,
};

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
            match self.registrations.entry(closure.signature().clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(ClosureRegistrationConstructor::from_closure_parameter(
                        class,
                        function.parameter(closure.call()).ty(),
                        closure,
                        false,
                        callbacks,
                    )?);
                }
                Entry::Occupied(_) => {}
            }
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
                match self.registrations.entry(closure.signature().clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert(ClosureRegistrationConstructor::from_closure_parameter(
                            class,
                            method.parameter(closure.call()).ty(),
                            closure,
                            true,
                            callbacks,
                        )?);
                    }
                    Entry::Occupied(mut entry) => {
                        let registration = entry.get_mut();
                        ClosureRegistrationConstructor::retain_callback_handle(
                            registration,
                            class,
                            method.parameter(closure.call()).ty(),
                        )?;
                    }
                }
            }
            c::ParameterGroup::ClosureReturn(returned) => {
                match self.registrations.entry(returned.signature().clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert(ClosureRegistrationConstructor::from_closure_return(
                            class, returned, callbacks,
                        )?);
                    }
                    Entry::Occupied(_) => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}

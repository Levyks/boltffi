//! Closure registrations referenced by return groups.
//!
//! Returned closures are represented as grouped C out-parameters. This module
//! extracts the closure signature from that group for the registration index.

use crate::{
    bridge::{c, jni::JvmClassPath},
    core::Result,
};

use super::super::build::ClosureRegistrationConstructor;
use super::ClosureRegistrationIndex;

impl ClosureRegistrationIndex {
    pub fn insert_closure_return(
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

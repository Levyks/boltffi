use std::collections::{BTreeMap, btree_map::Entry};

use boltffi_binding::ClosureSignature;

use crate::{
    bridge::{
        c::{self, Identifier},
        jni::{
            CallbackClosureHandle, ClosureArgument, ClosureRegistration, JvmClassPath,
            JvmMethodReturn,
        },
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

impl ClosureRegistration {
    /// Builds unique closure registrations from C functions and callback slots.
    pub fn from_c_bridge(
        class: &JvmClassPath,
        functions: &[c::Function],
        callbacks: &[c::Callback],
    ) -> Result<Vec<Self>> {
        functions
            .iter()
            .try_fold(BTreeMap::new(), |registrations, function| {
                Self::collect_function_closures(class, registrations, function)
            })
            .and_then(|registrations| {
                callbacks
                    .iter()
                    .flat_map(|callback| callback.methods().iter())
                    .try_fold(registrations, |registrations, slot| {
                        Self::collect_callback_closures(class, registrations, slot)
                    })
            })
            .map(BTreeMap::into_values)
            .map(Iterator::collect)
    }

    fn collect_function_closures(
        class: &JvmClassPath,
        registrations: BTreeMap<ClosureSignature, Self>,
        function: &c::Function,
    ) -> Result<BTreeMap<ClosureSignature, Self>> {
        function
            .parameter_groups()
            .iter()
            .try_fold(registrations, |registrations, group| {
                Self::insert_function_closure(class, registrations, function, group)
            })
    }

    fn collect_callback_closures(
        class: &JvmClassPath,
        registrations: BTreeMap<ClosureSignature, Self>,
        slot: &c::CallbackSlot,
    ) -> Result<BTreeMap<ClosureSignature, Self>> {
        slot.parameter_groups()
            .iter()
            .try_fold(registrations, |registrations, group| {
                Self::insert_callback_closure(class, registrations, slot, group)
            })
    }

    fn insert_function_closure(
        class: &JvmClassPath,
        mut registrations: BTreeMap<ClosureSignature, Self>,
        function: &c::Function,
        group: &c::ParameterGroup,
    ) -> Result<BTreeMap<ClosureSignature, Self>> {
        if let c::ParameterGroup::Closure(closure) = group {
            match registrations.entry(closure.signature().clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(Self::from_c_group(
                        class,
                        function.parameter(closure.call()).ty(),
                        closure,
                        false,
                    )?);
                }
                Entry::Occupied(_) => {}
            }
        }
        Ok(registrations)
    }

    fn insert_callback_closure(
        class: &JvmClassPath,
        mut registrations: BTreeMap<ClosureSignature, Self>,
        slot: &c::CallbackSlot,
        group: &c::ParameterGroup,
    ) -> Result<BTreeMap<ClosureSignature, Self>> {
        if let c::ParameterGroup::Closure(closure) = group {
            match registrations.entry(closure.signature().clone()) {
                Entry::Vacant(entry) => {
                    entry.insert(Self::from_c_group(
                        class,
                        slot.parameter(closure.call()).ty(),
                        closure,
                        true,
                    )?);
                }
                Entry::Occupied(mut entry) => {
                    entry
                        .get_mut()
                        .add_callback_handle(class, slot.parameter(closure.call()).ty())?;
                }
            }
        }
        Ok(registrations)
    }

    fn from_c_group(
        class: &JvmClassPath,
        call_type: &c::Type,
        closure: &c::ClosureParameter,
        callback_argument: bool,
    ) -> Result<Self> {
        let c::Type::FunctionPointer { returns, params } = call_type else {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure call parameter is not a function pointer",
            });
        };
        if !matches!(
            params.first(),
            Some(c::Type::MutPointer(inner)) if matches!(inner.as_ref(), c::Type::Void)
        ) {
            return Err(Error::BrokenBridgeContract {
                bridge: JNI_BRIDGE,
                invariant: "closure call parameter does not start with void context",
            });
        }
        let stem = closure.signature().symbol_part();
        Ok(Self {
            signature: closure.signature().clone(),
            class: class.closure_class(closure.signature())?,
            global_class: Identifier::parse(format!("g_{stem}_class"))?,
            call_method: Identifier::parse(format!("g_{stem}_call_method"))?,
            free_method: Identifier::parse(format!("g_{stem}_free_method"))?,
            load: Identifier::parse(format!("boltffi_jni_load_{stem}"))?,
            unload: Identifier::parse(format!("boltffi_jni_unload_{stem}"))?,
            call: Identifier::parse(format!("boltffi_jni_{stem}_call"))?,
            release: Identifier::parse(format!("boltffi_jni_{stem}_release"))?,
            callback_handle: callback_argument
                .then(|| CallbackClosureHandle::new(class, closure.signature(), call_type))
                .transpose()?,
            returns: JvmMethodReturn::from_c_type(returns)?,
            arguments: closure
                .parameter_groups()
                .iter()
                .map(|group| ClosureArgument::from_group(closure, group))
                .collect::<Result<Vec<_>>>()?,
        })
    }

    fn add_callback_handle(&mut self, class: &JvmClassPath, call_type: &c::Type) -> Result<()> {
        if self.callback_handle.is_none() {
            self.callback_handle = Some(CallbackClosureHandle::new(
                class,
                &self.signature,
                call_type,
            )?);
        }
        Ok(())
    }
}

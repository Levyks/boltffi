use std::collections::{BTreeMap, btree_map::Entry};

use boltffi_binding::ClosureSignature;

use crate::{
    bridge::{
        c::{self, Identifier, TypeFragment},
        jni::{ClosureArgument, JniReturn, JvmClassPath},
    },
    core::{Error, Result},
};

const JNI_BRIDGE: &str = "jni";

/// JNI trampoline registration for one inline closure signature.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct ClosureRegistration {
    signature: ClosureSignature,
    class: JvmClassPath,
    global_class: Identifier,
    call_method: Identifier,
    free_method: Identifier,
    load: Identifier,
    unload: Identifier,
    call: Identifier,
    release: Identifier,
    returns: JniReturn,
    arguments: Vec<ClosureArgument>,
}

impl ClosureRegistration {
    /// Builds unique closure trampoline registrations from C functions.
    pub fn from_functions(class: &JvmClassPath, functions: &[c::Function]) -> Result<Vec<Self>> {
        functions
            .iter()
            .try_fold(BTreeMap::new(), |registrations, function| {
                function.parameter_groups().iter().try_fold(
                    registrations,
                    |mut registrations, group| {
                        if let c::ParameterGroup::Closure(closure) = group {
                            match registrations.entry(closure.signature().clone()) {
                                Entry::Vacant(entry) => {
                                    entry.insert(Self::from_c_group(class, function, closure)?);
                                }
                                Entry::Occupied(_) => {}
                            }
                        }
                        Ok(registrations)
                    },
                )
            })
            .map(BTreeMap::into_values)
            .map(Iterator::collect)
    }

    /// Returns the closure invocation signature.
    pub fn signature(&self) -> &ClosureSignature {
        &self.signature
    }

    /// Returns the JVM closure bridge class.
    pub fn class(&self) -> &JvmClassPath {
        &self.class
    }

    /// Returns the global class reference symbol.
    pub fn global_class(&self) -> &Identifier {
        &self.global_class
    }

    /// Returns the cached static `call` method id symbol.
    pub fn call_method(&self) -> &Identifier {
        &self.call_method
    }

    /// Returns the cached static `free` method id symbol.
    pub fn free_method(&self) -> &Identifier {
        &self.free_method
    }

    /// Returns the load hook called from `JNI_OnLoad`.
    pub fn load(&self) -> &Identifier {
        &self.load
    }

    /// Returns the unload hook called from `JNI_OnUnload`.
    pub fn unload(&self) -> &Identifier {
        &self.unload
    }

    /// Returns the generated C closure invoke function.
    pub fn call(&self) -> &Identifier {
        &self.call
    }

    /// Returns the generated C closure release function.
    pub fn release(&self) -> &Identifier {
        &self.release
    }

    /// Returns the C return type for the closure invoke function.
    pub fn c_return_type(&self) -> &TypeFragment {
        self.returns.c_type()
    }

    /// Returns whether the closure invoke function returns no value.
    pub fn returns_void(&self) -> bool {
        self.returns.is_void()
    }

    /// Returns the JNI method descriptor.
    pub fn method_signature(&self) -> String {
        format!(
            "(J{}){}",
            self.arguments
                .iter()
                .map(ClosureArgument::jni_signature)
                .collect::<Vec<_>>()
                .join(""),
            self.returns.signature()
        )
    }

    /// Returns the `CallStatic*Method` suffix for non-void closure returns.
    pub fn call_method_suffix(&self) -> Option<&'static str> {
        self.returns.call_method_suffix()
    }

    /// Returns the C value returned when JVM dispatch fails.
    pub fn failure_value(&self) -> Option<&'static str> {
        self.returns.failure_value()
    }

    /// Returns generated C closure arguments.
    pub fn arguments(&self) -> &[ClosureArgument] {
        &self.arguments
    }

    fn from_c_group(
        class: &JvmClassPath,
        function: &c::Function,
        closure: &c::ClosureParameter,
    ) -> Result<Self> {
        let c::Type::FunctionPointer { returns, params } = function.parameter(closure.call()).ty()
        else {
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
            returns: JniReturn::from_c_type(returns)?,
            arguments: params
                .iter()
                .skip(1)
                .enumerate()
                .map(ClosureArgument::from_c_type)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

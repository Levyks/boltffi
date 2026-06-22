//! Native methods that call Rust-owned callback handles from the JVM.
//!
//! When Rust returns a callback handle to Java, Java receives only an opaque
//! `jlong`. Calling a method on that returned callback must go back through the
//! stored native vtable, not through the JVM callback implementation class used
//! for foreign callbacks.
//!
//! This module builds those handle-call methods from the same C callback slots
//! used by the lower bridge. Each method takes the handle token, unwraps the
//! stored callback, prepares the Java-provided arguments, and dispatches through
//! the callback vtable slot.
//!
//! Keeping this here matters because returned callbacks are not foreign
//! callbacks. A foreign callback is implemented by the JVM and handed to Rust.
//! A returned callback is owned by Rust and called later by the JVM. Both use the
//! same callback declaration, but the ownership and dispatch direction are
//! different.

use boltffi_binding::CallbackId;

use crate::{
    bridge::{
        c::{self, ArgumentList, Expression, Identifier, TypeFragment},
        jni::{ClosureRegistration, JniSymbolName, JvmClassPath, NativeParameter, NativeReturn},
    },
    core::Result,
};

/// JNI native method that invokes one method on a Rust-owned callback handle.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct CallbackHandleMethod {
    callback: CallbackId,
    symbol: JniSymbolName,
    vtable_type: Identifier,
    slot: Identifier,
    returns: NativeReturn,
    parameters: Vec<NativeParameter>,
}

impl CallbackHandleMethod {
    /// Builds handle-call methods for a returned callback trait.
    pub fn from_callback(
        class: &JvmClassPath,
        callback: &c::Callback,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Vec<Self>> {
        callback
            .methods()
            .iter()
            .filter(|slot| Self::supported(slot))
            .map(|slot| Self::from_slot(class, callback, slot, callbacks, closures))
            .collect()
    }

    /// Returns the source callback trait id.
    pub const fn callback(&self) -> CallbackId {
        self.callback
    }

    /// Returns the exported JNI symbol for this handle method.
    pub fn symbol(&self) -> &JniSymbolName {
        &self.symbol
    }

    /// Returns the C callback vtable type.
    pub fn vtable_type(&self) -> &Identifier {
        &self.vtable_type
    }

    /// Returns the callback vtable slot name.
    pub fn slot(&self) -> &Identifier {
        &self.slot
    }

    /// Returns the JNI return contract.
    pub fn returns(&self) -> &NativeReturn {
        &self.returns
    }

    /// Returns parameters after `JNIEnv*`, `jclass`, and callback handle.
    pub fn parameters(&self) -> &[NativeParameter] {
        &self.parameters
    }

    /// Returns whether this method returns no value.
    pub fn returns_void(&self) -> bool {
        matches!(&self.returns, NativeReturn::Void)
    }

    /// Returns whether this method needs an explicit `jboolean` cast.
    pub fn returns_boolean(&self) -> bool {
        matches!(&self.returns, NativeReturn::Value(scalar) if scalar.jni_type().is_boolean())
    }

    /// Returns whether this method returns an owned byte buffer.
    pub fn returns_bytes(&self) -> bool {
        matches!(&self.returns, NativeReturn::Bytes)
    }

    /// Returns whether this method returns a direct record byte array.
    pub fn returns_record(&self) -> bool {
        matches!(&self.returns, NativeReturn::Record(_))
    }

    /// Returns whether this method returns a callback handle token.
    pub fn returns_callback(&self) -> bool {
        self.returns.is_callback()
    }

    /// Returns whether this method checks a returned `FfiStatus`.
    pub fn checks_status(&self) -> bool {
        matches!(&self.returns, NativeReturn::Status)
    }

    fn from_slot(
        class: &JvmClassPath,
        callback: &c::Callback,
        slot: &c::CallbackSlot,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        let function = c::Function::new(
            Self::method_name(callback, slot.name()),
            slot.parameters().iter().skip(1).cloned().collect(),
            slot.returns().clone(),
        )?;
        Ok(Self {
            callback: callback.id(),
            symbol: JniSymbolName::native_method(class, function.name())?,
            vtable_type: Identifier::parse(callback.vtable().name())?,
            slot: slot.name().clone(),
            returns: NativeReturn::from_c_type(function.returns())?,
            parameters: NativeParameter::from_c_function(&function, callbacks, closures)?,
        })
    }

    fn supported(slot: &c::CallbackSlot) -> bool {
        !matches!(slot.returns(), c::Type::Status)
            && slot.parameter_groups().iter().all(|group| {
                !matches!(
                    group,
                    c::ParameterGroup::CallbackCompletion(_) | c::ParameterGroup::ClosureReturn(_)
                )
            })
    }

    fn method_name(callback: &c::Callback, slot: &Identifier) -> String {
        let callback = callback
            .create_handle()
            .name()
            .strip_prefix("boltffi_create_callback_")
            .unwrap_or_else(|| callback.create_handle().name());
        format!("boltffi_callback_handle_{callback}_{slot}")
    }

    /// Returns the C arguments passed to the callback vtable slot.
    pub fn arguments(&self) -> Result<ArgumentList> {
        Ok(ArgumentList::from_iter(
            [Expression::new("callback_handle->handle")]
                .into_iter()
                .chain(
                    self.parameters
                        .iter()
                        .map(NativeParameter::c_arguments)
                        .collect::<Result<Vec<_>>>()?
                        .into_iter()
                        .flatten(),
                ),
        ))
    }

    /// Returns the expression returned from the JNI method.
    pub fn return_value(&self, value: Expression) -> Result<Expression> {
        self.returns.return_expression(value)
    }

    /// Returns the JNI method return type as C syntax.
    pub fn jni_type(&self) -> TypeFragment {
        self.returns.jni_type()
    }

    /// Returns the temporary C result type used inside the method body.
    pub fn c_result_type(&self) -> Result<TypeFragment> {
        self.returns.c_result_type()
    }
}

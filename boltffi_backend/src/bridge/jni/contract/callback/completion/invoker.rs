//! Native invokers for async callback completion.
//!
//! Async callback methods do not return their payload directly. The JVM calls a
//! generated success or failure native method later; this module deduplicates
//! those invokers by payload shape.

use std::collections::{BTreeMap, btree_map::Entry};

use crate::{
    bridge::jni::{CallbackCompletionPayload, CallbackRegistration, JniSymbolName, JvmClassPath},
    core::Result,
};

/// JNI native methods that complete an async callback invocation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub struct CallbackCompletionInvoker {
    success: JniSymbolName,
    failure: JniSymbolName,
    payload: Option<CallbackCompletionPayload>,
}

impl CallbackCompletionInvoker {
    /// Builds the distinct completion invokers needed by registered callback traits.
    pub fn from_callbacks(
        class: &JvmClassPath,
        callbacks: &[CallbackRegistration],
    ) -> Result<Vec<Self>> {
        callbacks
            .iter()
            .flat_map(CallbackRegistration::methods)
            .flat_map(|method| method.completions().into_iter())
            .try_fold(BTreeMap::new(), |mut invokers, completion| {
                let key = completion
                    .payload()
                    .map_or_else(|| "Void".to_owned(), |payload| payload.suffix().to_owned());
                match invokers.entry(key.clone()) {
                    Entry::Vacant(entry) => {
                        entry.insert(Self::new(class, &key, completion.payload().cloned())?);
                    }
                    Entry::Occupied(_) => {}
                }
                Ok::<_, crate::core::Error>(invokers)
            })
            .map(BTreeMap::into_values)
            .map(Iterator::collect)
    }

    /// Returns the success native method symbol.
    pub fn success(&self) -> &JniSymbolName {
        &self.success
    }

    /// Returns the failure native method symbol.
    pub fn failure(&self) -> &JniSymbolName {
        &self.failure
    }

    /// Returns the successful completion payload shape.
    pub fn payload(&self) -> Option<&CallbackCompletionPayload> {
        self.payload.as_ref()
    }

    fn new(
        class: &JvmClassPath,
        suffix: &str,
        payload: Option<CallbackCompletionPayload>,
    ) -> Result<Self> {
        Ok(Self {
            success: JniSymbolName::native_method(
                class,
                &format!("boltffi_async_callback_complete_{suffix}"),
            )?,
            failure: JniSymbolName::native_method(
                class,
                &format!("boltffi_async_callback_complete_{suffix}_failure"),
            )?,
            payload,
        })
    }
}

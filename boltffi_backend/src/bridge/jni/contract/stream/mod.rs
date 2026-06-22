mod direct_batch;

use crate::{
    bridge::{
        c,
        jni::{ClosureRegistration, JvmClassPath, NativeMethod},
    },
    core::Result,
};

pub use direct_batch::DirectStreamBatchMethod;

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
/// JNI methods generated for one C stream protocol.
pub struct StreamProtocolMethods {
    methods: Vec<NativeMethod>,
    direct_batches: Vec<DirectStreamBatchMethod>,
}

impl StreamProtocolMethods {
    /// Creates JNI stream methods from a C stream protocol.
    pub fn from_c_stream(
        class: &JvmClassPath,
        stream: &c::Stream,
        callbacks: &[c::Callback],
        closures: &[ClosureRegistration],
    ) -> Result<Self> {
        let direct_batch_name = stream
            .direct_batch()
            .map(|batch| batch.function().name().to_owned());
        let methods = stream
            .functions()
            .into_iter()
            .filter(|function| Some(function.name()) != direct_batch_name.as_deref())
            .map(|function| NativeMethod::new(class, function, callbacks, closures))
            .collect::<Result<Vec<_>>>()?;
        let direct_batches = stream
            .direct_batch()
            .map(|batch| DirectStreamBatchMethod::from_c_batch(class, batch))
            .transpose()?
            .into_iter()
            .collect();

        Ok(Self {
            methods,
            direct_batches,
        })
    }

    /// Returns stream protocol methods rendered by the generic JNI method path.
    pub fn methods(&self) -> &[NativeMethod] {
        &self.methods
    }

    /// Returns direct batch methods that need stream-specific JNI rendering.
    pub fn direct_batches(&self) -> &[DirectStreamBatchMethod] {
        &self.direct_batches
    }
}

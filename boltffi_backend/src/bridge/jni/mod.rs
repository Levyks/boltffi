//! JNI bridge.
//!
//! This bridge layers above the C ABI bridge. It emits C functions with
//! JNI-exported names and gives JVM hosts a typed native-method contract.

mod bridge;
mod contract;
mod name;
mod template;

pub use bridge::JniBridge;
pub use contract::{
    BytesParameter, CallbackArgument, CallbackBytesArgument, CallbackCParameter,
    CallbackClosureArgument, CallbackClosureHandle, CallbackCompletionArgument,
    CallbackDirectVectorArgument, CallbackHandleArgument, CallbackMethod, CallbackParameter,
    CallbackRecordArgument, CallbackRegistration, CallbackReturn, ClosureArgument,
    ClosureParameter, ClosureRegistration, ContinuationParameter, DirectVectorParameter,
    JniBridgeContract, JniType, JvmMethodReturn, NativeMethod, NativeParameter,
    NativeParameterKind, NativeReturn, RecordParameter, RecordValue, ScalarParameter, ScalarReturn,
};
pub use name::{JniSymbolName, JvmClassPath, JvmNameSegment};

#[cfg(test)]
mod tests;

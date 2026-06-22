//! JNI bridge layered above the C ABI bridge.
//!
//! The C bridge is the stable Rust-facing ABI. It knows exported symbols,
//! buffer ownership, callback vtables, stream protocols, async continuations,
//! and direct-record layouts. That contract is enough for C callers. It is not
//! enough for Java or Kotlin, because the JVM needs `Java_*` entry points, JVM
//! descriptors, Java arrays, global references, cached method ids, and cleanup
//! tied to a live `JNIEnv`.
//!
//! This bridge is the shared JNI layer for JVM targets. Java, Kotlin, and any
//! later JVM host can stand on the same native bridge instead of rebuilding JNI
//! callback dispatch, byte-array borrowing, closure trampolines, and stream
//! helpers independently. It adapts one complete `CBridgeContract` into typed
//! JVM-facing facts, then renders one generated C source file.
//!
//! The flow is deliberately narrow. `JniBridge` receives the C bridge contract.
//! `contract` turns it into JNI method, callback, closure, stream, and name
//! contracts. `name` owns JVM class paths and `Java_*` symbol spelling.
//! `template` prints the final source through Askama. None of those layers lower
//! `Bindings` again, inspect Rust source, or decide encoded transport locally.

mod bridge;
mod contract;
mod name;
mod template;

pub use bridge::JniBridge;
pub use contract::{
    BytesParameter, CallbackArgument, CallbackBytesArgument, CallbackCParameter,
    CallbackClosureArgument, CallbackClosureHandle, CallbackClosureReturn,
    CallbackCompletionArgument, CallbackCompletionInvoker, CallbackCompletionPayload,
    CallbackDirectVectorArgument, CallbackHandleArgument, CallbackHandleCompletion,
    CallbackHandleMethod, CallbackMethod, CallbackParameter, CallbackRecordArgument,
    CallbackRegistration, CallbackReturn, ClosureArgument, ClosureBytesArgument, ClosureCParameter,
    ClosureDirectVectorArgument, ClosureHandleArgument, ClosureParameter, ClosureRegistration,
    ContinuationParameter, DirectStreamBatchMethod, DirectVectorParameter, DirectVectorStackCopy,
    JniBridgeContract, JniType, JvmMethodReturn, NativeMethod, NativeParameter,
    NativeParameterKind, NativeReturn, RecordParameter, RecordValue, ScalarParameter, ScalarReturn,
};
pub use name::{JniSymbolName, JvmClassPath, JvmNameSegment};

#[cfg(test)]
mod tests;

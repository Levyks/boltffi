//! JVM bridge layered above the C ABI bridge.
//!
//! The C bridge is the stable Rust-facing ABI. It knows the exported Rust
//! symbols, buffer ownership rules, callback vtables, stream protocols, async
//! continuations, and direct-record layouts. That contract is enough for C, but
//! not for the JVM. A JVM caller needs `Java_*` entry points, JVM descriptors,
//! Java arrays, global references, cached method ids, and cleanup tied to a live
//! `JNIEnv`.
//!
//! This bridge exists so Java and Kotlin targets do not each rebuild that JNI
//! layer. It adapts one complete `CBridgeContract` into typed JVM-facing facts,
//! then renders one generated C source file. It does not lower `Bindings` again,
//! inspect Rust source, or decide transport rules locally.
//!
//! The flow is deliberately narrow. `JniBridge` receives the C bridge contract,
//! `contract` turns it into JNI method, callback, closure, stream, and name
//! contracts, `name` owns JVM and JNI spelling, and `template` prints the final
//! source through Askama. That keeps callback ownership, byte-array borrowing,
//! stream helper names, and `Java_*` escaping in one bridge instead of spreading
//! them across every JVM host backend.

mod bridge;
mod contract;
mod name;
mod template;

pub use bridge::JniBridge;
pub use contract::{
    BytesParameter, CallbackArgument, CallbackBytesArgument, CallbackCParameter,
    CallbackClosureArgument, CallbackClosureHandle, CallbackClosureReturn,
    CallbackCompletionArgument, CallbackCompletionInvoker, CallbackCompletionPayload,
    CallbackDirectVectorArgument, CallbackHandleArgument, CallbackHandleMethod, CallbackMethod,
    CallbackParameter, CallbackRecordArgument, CallbackRegistration, CallbackReturn,
    ClosureArgument, ClosureBytesArgument, ClosureCParameter, ClosureDirectVectorArgument,
    ClosureHandleArgument, ClosureParameter, ClosureRegistration, ContinuationParameter,
    DirectStreamBatchMethod, DirectVectorParameter, DirectVectorStackCopy, JniBridgeContract,
    JniType, JvmMethodReturn, NativeMethod, NativeParameter, NativeParameterKind, NativeReturn,
    RecordParameter, RecordValue, ScalarParameter, ScalarReturn,
};
pub use name::{JniSymbolName, JvmClassPath, JvmNameSegment};

#[cfg(test)]
mod tests;

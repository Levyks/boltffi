//! JVM bridge layered above the C ABI bridge.
//!
//! The C bridge is the stable Rust-facing ABI. It knows the exported symbols,
//! buffer ownership rules, callback vtables, stream protocols, async
//! continuations, and direct-record layouts. That is enough for a C caller, but
//! it is not enough for a JVM caller. JNI needs `Java_*` entry points, JVM method
//! descriptors, Java arrays, global references, cached method ids, and cleanup
//! that is tied to a live `JNIEnv`.
//!
//! This bridge exists to adapt one complete C bridge contract into that JVM
//! shape. It does not lower `Bindings` again and it does not inspect Rust source.
//! The flow is deliberately narrow: `JniBridge` receives a `CBridgeContract`,
//! `contract` turns it into typed JNI facts, `name` owns JVM and JNI spelling,
//! and `template` prints the final C source through Askama.
//!
//! Keeping this layer separate gives Java and Kotlin targets one shared JNI
//! bridge instead of forcing each host language to rediscover callback
//! ownership, byte-array borrowing, stream helper names, or `Java_*` symbol
//! escaping.

mod bridge;
mod contract;
mod name;
mod template;

pub use bridge::JniBridge;
pub use contract::{
    BytesParameter, CallbackArgument, CallbackBytesArgument, CallbackCParameter,
    CallbackClosureArgument, CallbackClosureHandle, CallbackClosureReturn,
    CallbackCompletionArgument, CallbackCompletionInvoker, CallbackCompletionPayload,
    CallbackDirectVectorArgument, CallbackHandleArgument, CallbackMethod, CallbackParameter,
    CallbackRecordArgument, CallbackRegistration, CallbackReturn, ClosureArgument,
    ClosureBytesArgument, ClosureCParameter, ClosureDirectVectorArgument, ClosureHandleArgument,
    ClosureParameter, ClosureRegistration, ContinuationParameter, DirectStreamBatchMethod,
    DirectVectorParameter, JniBridgeContract, JniType, JvmMethodReturn, NativeMethod,
    NativeParameter, NativeParameterKind, NativeReturn, RecordParameter, RecordValue,
    ScalarParameter, ScalarReturn,
};
pub use name::{JniSymbolName, JvmClassPath, JvmNameSegment};

#[cfg(test)]
mod tests;

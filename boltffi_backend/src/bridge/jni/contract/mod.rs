//! Typed JNI contract built from the lower C bridge contract.
//!
//! The C bridge has already decided how Rust is called. It knows the exported C
//! functions, the grouped parameters they accept, the return slots they fill,
//! the callback vtables Rust calls, and the stream protocol functions. JNI needs
//! the same contract in JVM terms: Java parameter types, JNI descriptors,
//! borrowed array lifetimes, callback method ids, `Java_*` symbols, and cleanup
//! paths tied to `JNIEnv`.
//!
//! This module is the adaptation boundary. It reads the C bridge contract,
//! validates that every C shape has a JVM representation, and stores the result
//! as typed values. Rendering code consumes those values. It does not inspect
//! `TypeRef`, re-walk codec plans, or rebuild parameter groups from raw C
//! fragments.
//!
//! The child modules are split by the thing they own. `parameter` groups C
//! arguments into Java parameters. `return_value` describes what Java receives.
//! `callback` and `closure` model the two callback directions. `stream` keeps
//! stream protocols together. `record`, `scalar`, `bytes`, and `direct_vector`
//! own the reusable ABI shapes shared by those paths.

mod bridge;
mod bytes;
mod callback;
mod closure;
mod continuation;
mod direct_vector;
mod jni_type;
mod jvm;
mod method;
mod parameter;
mod record;
mod return_value;
mod scalar;
mod stream;

pub use bridge::JniBridgeContract;
pub use bytes::BytesParameter;
pub use callback::{
    CallbackArgument, CallbackBytesArgument, CallbackCParameter, CallbackClosureArgument,
    CallbackClosureReturn, CallbackCompletionArgument, CallbackCompletionInvoker,
    CallbackCompletionPayload, CallbackDirectVectorArgument, CallbackHandleArgument,
    CallbackHandleMethod, CallbackMethod, CallbackParameter, CallbackRecordArgument,
    CallbackRegistration, CallbackReturn,
};
pub use closure::{
    CallbackClosureHandle, ClosureArgument, ClosureBytesArgument, ClosureCParameter,
    ClosureDirectVectorArgument, ClosureHandleArgument, ClosureParameter, ClosureRegistration,
};
pub use continuation::ContinuationParameter;
pub use direct_vector::{DirectVectorParameter, DirectVectorStackCopy};
pub use jni_type::JniType;
pub use jvm::JvmMethodReturn;
pub use method::NativeMethod;
pub use parameter::{NativeParameter, NativeParameterKind};
pub use record::{RecordParameter, RecordValue};
pub use return_value::NativeReturn;
pub use scalar::{ScalarParameter, ScalarReturn};
pub use stream::{DirectStreamBatchMethod, StreamProtocolMethods};

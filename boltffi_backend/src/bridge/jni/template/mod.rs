//! Askama rendering for the generated JNI C source.
//!
//! The contract layer decides what exists and what each piece means. This layer
//! turns that contract into source-shaped template data: declarations, local
//! variables, JNI calls, cleanup labels, and return expressions. Large generated
//! C syntax stays in Askama templates, while Rust keeps the values typed before
//! they reach those templates.
//!
//! The split is intentional. Rust prepares typed values; Askama owns the shape of
//! the generated C text. A template may format a callback method, stream helper,
//! closure trampoline, or native method, but it does not decide whether a value
//! is encoded, direct, async, fallible, or borrowed. Those decisions already live
//! in the JNI contract.

mod callback;
mod closure;
mod method;
mod source;
mod stream;

pub use self::source::SourceFile;

use super::error::Error;
use super::target::Target;

mod local;

pub mod asynchronous;
pub mod callable;
mod codec;
pub mod function;
pub mod handle;
pub mod param;
pub mod returns;
pub mod type_ref;

/// A render rule for one typed expansion input.
///
/// The `(S, Input)` pair selects the implementation. Generic `S: Target`
/// implementations represent ABI behavior shared by all targets; concrete
/// `Native` or `Wasm32` implementations represent surface-specific ABI behavior.
///
/// # Example
///
/// ```rust,ignore
/// struct DirectRecord;
///
/// impl Rule<Native, RecordInput> for DirectRecord {
///     type Output = Tokens;
///
///     fn apply(self, input: RecordInput) -> Result<Tokens, Error> {
///         Tokens::native_passable(input)
///     }
/// }
///
/// impl Rule<Wasm32, RecordInput> for DirectRecord {
///     type Output = Tokens;
///
///     fn apply(self, input: RecordInput) -> Result<Tokens, Error> {
///         Tokens::wasm_memory_pointer(input)
///     }
/// }
/// ```
pub trait Rule<S: Target, Input> {
    /// The token fragment or typed intermediate value produced by the rule.
    type Output;

    /// Renders one input value for target surface `S`.
    fn apply(self, input: Input) -> Result<Self::Output, Error>;
}

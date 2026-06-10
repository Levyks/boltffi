use super::error::Error;
use super::target::Target;

mod names;

pub mod arguments;
pub mod async_call;
pub mod callback;
pub mod encoded;
pub mod function;
pub mod handle;
pub mod param;
pub mod returns;
pub mod type_ref;

mod closure;

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
/// impl Render<Native, RecordInput> for DirectRecord {
///     type Output = Tokens;
///
///     fn render(self, input: RecordInput) -> Result<Tokens, Error> {
///         Tokens::native_passable(input)
///     }
/// }
///
/// impl Render<Wasm32, RecordInput> for DirectRecord {
///     type Output = Tokens;
///
///     fn render(self, input: RecordInput) -> Result<Tokens, Error> {
///         Tokens::wasm_memory_pointer(input)
///     }
/// }
/// ```
pub trait Render<S: Target, Input> {
    /// The token fragment or typed intermediate value produced by the rule.
    type Output;

    /// Renders one input value for target surface `S`.
    fn render(self, input: Input) -> Result<Self::Output, Error>;
}

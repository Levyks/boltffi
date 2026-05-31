use super::error::Error;
use super::target::Target;

pub mod callable;
pub mod function;
pub mod param;
pub mod returns;
pub mod type_ref;

/// A render step for one lowered binding fragment.
///
/// A rule receives a typed IR value, such as an exported callable, a
/// parameter, a return declaration, or a type reference. It returns either
/// final Rust tokens or a typed fragment that a larger rule can combine into
/// final Rust syntax.
pub trait Rule<S: Target, Input> {
    type Output;

    fn apply(self, input: Input) -> Result<Self::Output, Error>;
}

mod body;
mod function;
mod member;
mod parameter;
mod return_value;

pub use self::{function::FunctionStub, member::AssociatedCallable, return_value::ReturnStub};

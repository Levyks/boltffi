use boltffi_binding::{OutOfRust, ReturnDecl, ReturnPlan, TypeRef};
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::{
    error::Error,
    render::{self, Rule as RenderRule},
    target::Target,
};

pub struct Rule;

impl<'a, S: Target> RenderRule<S, &'a ReturnDecl<S, OutOfRust>> for Rule {
    type Output = TokenStream;

    fn apply(self, returns: &'a ReturnDecl<S, OutOfRust>) -> Result<Self::Output, Error> {
        match returns.plan() {
            ReturnPlan::Void => Ok(TokenStream::new()),
            ReturnPlan::DirectViaReturnSlot {
                ty: TypeRef::Primitive(primitive),
            } => {
                let ty = TypeRef::Primitive(*primitive);
                let ty = <render::type_ref::Rule as RenderRule<S, &TypeRef>>::apply(
                    render::type_ref::Rule,
                    &ty,
                )?;
                Ok(quote! { -> #ty })
            }
            ReturnPlan::DirectViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("non-primitive direct return"))
            }
            ReturnPlan::EncodedViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("encoded return"))
            }
            ReturnPlan::HandleViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("handle return"))
            }
            ReturnPlan::ScalarOptionViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("scalar option return"))
            }
            ReturnPlan::DirectVecViaReturnSlot { .. } => {
                Err(Error::UnsupportedExpansion("direct vec return"))
            }
            ReturnPlan::DirectViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("direct out-pointer return"))
            }
            ReturnPlan::EncodedViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("encoded out-pointer return"))
            }
            ReturnPlan::HandleViaOutPointer { .. } => {
                Err(Error::UnsupportedExpansion("handle out-pointer return"))
            }
            ReturnPlan::ClosureViaOutPointer(_) => {
                Err(Error::UnsupportedExpansion("closure out-pointer return"))
            }
            _ => Err(Error::UnsupportedExpansion("unknown return")),
        }
    }
}

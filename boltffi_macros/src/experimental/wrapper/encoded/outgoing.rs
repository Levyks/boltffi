use boltffi_binding::CodecNode;
use proc_macro2::TokenStream;
use quote::quote;

use crate::experimental::error::Error;

pub struct Value<'a> {
    root: &'a CodecNode,
}

impl<'a> Value<'a> {
    pub const fn new(root: &'a CodecNode) -> Self {
        Self { root }
    }

    pub fn buffer(&self, value: TokenStream) -> Result<TokenStream, Error> {
        super::require_runtime_wire(self.root)?;
        Ok(quote! { ::boltffi::__private::FfiBuf::wire_encode(&#value) })
    }
}

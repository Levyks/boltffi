use boltffi_binding::{Native, Surface, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

pub trait Target: Surface {
    fn cfg_attr() -> TokenStream;
}

impl Target for Native {
    fn cfg_attr() -> TokenStream {
        quote! { #[cfg(not(target_arch = "wasm32"))] }
    }
}

impl Target for Wasm32 {
    fn cfg_attr() -> TokenStream {
        quote! { #[cfg(target_arch = "wasm32")] }
    }
}

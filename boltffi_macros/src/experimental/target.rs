use boltffi_binding::{Native, Surface, Wasm32};
use proc_macro2::TokenStream;
use quote::quote;

/// A binding surface that can gate generated Rust wrapper items.
///
/// Binding lowering already defines the ABI contract for the surface. Expansion adds the
/// Rust syntax needed around emitted wrappers, such as the `cfg` attribute that keeps native
/// and wasm exports from compiling into the same target artifact.
///
/// # Example
///
/// ```rust,ignore
/// let native_cfg = Native::cfg_attr();
/// let wasm_cfg = Wasm32::cfg_attr();
///
/// quote! {
///     #native_cfg
///     #[unsafe(no_mangle)]
///     pub extern "C" fn boltffi_function_demo_add() -> u32 {
///         add()
///     }
///
///     #wasm_cfg
///     #[unsafe(no_mangle)]
///     pub extern "C" fn boltffi_function_demo_add() -> u32 {
///         add()
///     }
/// }
/// ```
pub trait Target: Surface {
    /// Returns the `cfg` attribute applied to generated wrapper items for this surface.
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

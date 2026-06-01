use syn::{FnArg, Pat};

mod callbacks;
mod transform;
mod value;

use self::callbacks::{AsyncCallbackParamLowerer, SyncCallbackParamLowerer, TraitObjectParamKind};
use self::transform::{ClassifiedParamTransform, ParamTransform, ParamTransformClassifier};
use self::value::{AsyncValueParamLowerer, SyncValueParamLowerer};
use crate::index::callback_traits::CallbackTraitRegistry;
use crate::lowering::returns::model::ReturnLoweringContext;
use boltffi_ffi_rules::transport::ParamValueStrategy;

pub struct FfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub conversions: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
}

pub struct AsyncFfiParams {
    pub ffi_params: Vec<proc_macro2::TokenStream>,
    pub pre_spawn: Vec<proc_macro2::TokenStream>,
    pub thread_setup: Vec<proc_macro2::TokenStream>,
    pub call_args: Vec<proc_macro2::TokenStream>,
    pub move_vars: Vec<syn::Ident>,
}

pub(super) struct ParamLoweringState {
    pub(super) ffi_params: Vec<proc_macro2::TokenStream>,
    pub(super) setup: Vec<proc_macro2::TokenStream>,
    pub(super) thread_setup: Vec<proc_macro2::TokenStream>,
    pub(super) call_args: Vec<proc_macro2::TokenStream>,
    pub(super) move_vars: Vec<syn::Ident>,
}

impl ParamLoweringState {
    fn new() -> Self {
        Self {
            ffi_params: Vec::new(),
            setup: Vec::new(),
            thread_setup: Vec::new(),
            call_args: Vec::new(),
            move_vars: Vec::new(),
        }
    }

    fn into_sync(self) -> FfiParams {
        FfiParams {
            ffi_params: self.ffi_params,
            conversions: self.setup,
            call_args: self.call_args,
        }
    }

    fn into_async(self) -> AsyncFfiParams {
        AsyncFfiParams {
            ffi_params: self.ffi_params,
            pre_spawn: self.setup,
            thread_setup: self.thread_setup,
            call_args: self.call_args,
            move_vars: self.move_vars,
        }
    }
}

struct SyncParamLowerer<'a> {
    callback_param_lowerer: SyncCallbackParamLowerer<'a>,
    param_transform_classifier: ParamTransformClassifier<'a>,
    value_param_lowerer: SyncValueParamLowerer<'a>,
}

impl<'a> SyncParamLowerer<'a> {
    fn new(
        return_lowering: &'a ReturnLoweringContext<'a>,
        callback_registry: &'a CallbackTraitRegistry,
        on_wire_record_error: &'a proc_macro2::TokenStream,
    ) -> Self {
        Self {
            callback_param_lowerer: SyncCallbackParamLowerer::new(
                return_lowering,
                callback_registry,
            ),
            param_transform_classifier: ParamTransformClassifier::new(
                return_lowering.named_type_transport_classifier(),
            ),
            value_param_lowerer: SyncValueParamLowerer::new(
                return_lowering.custom_types(),
                on_wire_record_error,
            ),
        }
    }

    fn lower_inputs(
        &self,
        inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    ) -> ParamLoweringState {
        inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                FnArg::Receiver(_) => None,
            })
            .fold(ParamLoweringState::new(), |mut acc, pat_type| {
                let Some(name) = (match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => Some(ident.ident.clone()),
                    _ => None,
                }) else {
                    return acc;
                };

                let classified_param = self.param_transform_classifier.classify(&pat_type.ty);

                match classified_param.contract.value_strategy() {
                    ParamValueStrategy::CallbackHandle { .. } => match classified_param.transform {
                        ParamTransform::Callback {
                            params: arg_types,
                            returns,
                        } => self
                            .callback_param_lowerer
                            .lower_callback_param(&mut acc, &name, &arg_types, &returns),
                        ParamTransform::BoxedDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::Boxed,
                            )
                        }
                        ParamTransform::ArcDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::Arc,
                            )
                        }
                        ParamTransform::OptionBoxedDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::OptionBoxed,
                            )
                        }
                        ParamTransform::OptionArcDynTrait(trait_path) => {
                            self.callback_param_lowerer.lower_trait_object_param(
                                &mut acc,
                                &name,
                                &trait_path,
                                TraitObjectParamKind::OptionArc,
                            )
                        }
                        ParamTransform::ImplTrait(trait_path) => self
                            .callback_param_lowerer
                            .lower_impl_trait_param(&mut acc, &name, &trait_path),
                        param_transform => {
                            unreachable!(
                                "callback param contract must lower through callback builders: {:?}",
                                param_transform_name(&param_transform)
                            )
                        }
                    },
                    _ => self.value_param_lowerer.lower_param_transform(
                        &mut acc,
                        &name,
                        &pat_type.ty,
                        classified_param.transform,
                    ),
                }

                acc
            })
    }
}

struct AsyncParamLowerer<'a> {
    callback_param_lowerer: AsyncCallbackParamLowerer<'a>,
    param_transform_classifier: ParamTransformClassifier<'a>,
    value_param_lowerer: AsyncValueParamLowerer<'a>,
}

impl<'a> AsyncParamLowerer<'a> {
    fn new(
        return_lowering: &'a ReturnLoweringContext<'a>,
        callback_registry: &'a CallbackTraitRegistry,
        on_wire_record_error: &'a proc_macro2::TokenStream,
    ) -> Self {
        Self {
            callback_param_lowerer: AsyncCallbackParamLowerer::new(
                return_lowering,
                callback_registry,
            ),
            param_transform_classifier: ParamTransformClassifier::new(
                return_lowering.named_type_transport_classifier(),
            ),
            value_param_lowerer: AsyncValueParamLowerer::new(
                return_lowering.custom_types(),
                on_wire_record_error,
            ),
        }
    }

    fn validate_inputs(
        &self,
        inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    ) -> syn::Result<()> {
        inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                FnArg::Receiver(_) => None,
            })
            .filter_map(|pat_type| {
                let classified_param = self.param_transform_classifier.classify(&pat_type.ty);
                Self::unsupported_param_message(&classified_param)
                    .map(|message| syn::Error::new_spanned(&pat_type.ty, message))
            })
            .reduce(|mut left, right| {
                left.combine(right);
                left
            })
            .map_or(Ok(()), Err)
    }

    fn unsupported_param_message(
        classified_param: &ClassifiedParamTransform,
    ) -> Option<&'static str> {
        match &classified_param.transform {
            ParamTransform::Callback { .. } => {
                Some("boltffi: async exports do not support closure callback parameters yet")
            }
            ParamTransform::SliceMut(_) => {
                Some("boltffi: async exports do not support mutable slice parameters (`&mut [T]`)")
            }
            ParamTransform::BoxedDynTrait(_)
            | ParamTransform::ArcDynTrait(_)
            | ParamTransform::OptionBoxedDynTrait(_)
            | ParamTransform::OptionArcDynTrait(_) => Some(
                "boltffi: async exports do not support trait object callback parameters (`Box<dyn Trait>`, `Arc<dyn Trait>`, `Option<Box<dyn Trait>>`, `Option<Arc<dyn Trait>>`) yet",
            ),
            ParamTransform::StrRef
            | ParamTransform::StringRef
            | ParamTransform::OwnedString
            | ParamTransform::SliceRef(_)
            | ParamTransform::VecPrimitive(_)
            | ParamTransform::VecPassable(_)
            | ParamTransform::WireEncoded(_)
            | ParamTransform::Passable(_)
            | ParamTransform::ImplTrait(_)
            | ParamTransform::PassThrough => None,
        }
    }

    fn lower_inputs(
        &self,
        inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    ) -> ParamLoweringState {
        inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(pat_type) => Some(pat_type),
                FnArg::Receiver(_) => None,
            })
            .fold(ParamLoweringState::new(), |mut acc, pat_type| {
                let Some(name) = (match pat_type.pat.as_ref() {
                    Pat::Ident(ident) => Some(ident.ident.clone()),
                    _ => None,
                }) else {
                    return acc;
                };

                let classified_param = self.param_transform_classifier.classify(&pat_type.ty);

                match classified_param.contract.value_strategy() {
                    ParamValueStrategy::CallbackHandle { .. } => match classified_param.transform {
                        ParamTransform::ImplTrait(trait_path) => self
                            .callback_param_lowerer
                            .lower_impl_trait_param(&mut acc, &name, &trait_path),
                        ParamTransform::Callback { .. }
                        | ParamTransform::BoxedDynTrait(_)
                        | ParamTransform::ArcDynTrait(_)
                        | ParamTransform::OptionBoxedDynTrait(_)
                        | ParamTransform::OptionArcDynTrait(_)
                        | ParamTransform::SliceMut(_) => {
                            unreachable!(
                                "unsupported async params must be rejected during validation"
                            );
                        }
                        param_transform => {
                            unreachable!(
                                "callback param contract must lower through callback builders: {:?}",
                                param_transform_name(&param_transform)
                            )
                        }
                    },
                    _ => self.value_param_lowerer.lower_param_transform(
                        &mut acc,
                        &name,
                        &pat_type.ty,
                        classified_param.transform,
                    ),
                }

                acc
            })
    }
}

fn param_transform_name(param_transform: &ParamTransform) -> &'static str {
    match param_transform {
        ParamTransform::PassThrough => "PassThrough",
        ParamTransform::StrRef => "StrRef",
        ParamTransform::StringRef => "StringRef",
        ParamTransform::OwnedString => "OwnedString",
        ParamTransform::Callback { .. } => "Callback",
        ParamTransform::SliceRef(_) => "SliceRef",
        ParamTransform::SliceMut(_) => "SliceMut",
        ParamTransform::BoxedDynTrait(_) => "BoxedDynTrait",
        ParamTransform::ArcDynTrait(_) => "ArcDynTrait",
        ParamTransform::OptionBoxedDynTrait(_) => "OptionBoxedDynTrait",
        ParamTransform::OptionArcDynTrait(_) => "OptionArcDynTrait",
        ParamTransform::ImplTrait(_) => "ImplTrait",
        ParamTransform::VecPrimitive(_) => "VecPrimitive",
        ParamTransform::VecPassable(_) => "VecPassable",
        ParamTransform::WireEncoded(_) => "WireEncoded",
        ParamTransform::Passable(_) => "Passable",
    }
}

pub fn transform_params(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> FfiParams {
    SyncParamLowerer::new(return_lowering, callback_registry, on_wire_record_error)
        .lower_inputs(inputs)
        .into_sync()
}

pub fn transform_params_async(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> syn::Result<AsyncFfiParams> {
    let async_param_lowerer =
        AsyncParamLowerer::new(return_lowering, callback_registry, on_wire_record_error);
    async_param_lowerer.validate_inputs(inputs)?;
    Ok(async_param_lowerer.lower_inputs(inputs).into_async())
}

pub fn transform_method_params(
    inputs: impl Iterator<Item = syn::FnArg>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> FfiParams {
    let function_like_inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = inputs.collect();
    transform_params(
        &function_like_inputs,
        return_lowering,
        callback_registry,
        on_wire_record_error,
    )
}

pub fn transform_method_params_async(
    inputs: impl Iterator<Item = syn::FnArg>,
    return_lowering: &ReturnLoweringContext<'_>,
    callback_registry: &CallbackTraitRegistry,
    on_wire_record_error: &proc_macro2::TokenStream,
) -> syn::Result<AsyncFfiParams> {
    let function_like_inputs: syn::punctuated::Punctuated<FnArg, syn::Token![,]> = inputs.collect();
    transform_params_async(
        &function_like_inputs,
        return_lowering,
        callback_registry,
        on_wire_record_error,
    )
}

#[cfg(test)]
mod tests {
    use super::AsyncParamLowerer;
    use crate::index::callback_traits::CallbackTraitRegistry;
    use crate::index::custom_types::CustomTypeRegistry;
    use crate::index::data_types::DataTypeRegistry;
    use crate::lowering::returns::model::ReturnLoweringContext;
    use syn::parse_quote;

    fn async_param_lowerer() -> AsyncParamLowerer<'static> {
        let custom_types = Box::leak(Box::new(CustomTypeRegistry::default()));
        let data_types = Box::leak(Box::new(DataTypeRegistry::default()));
        let return_lowering = Box::leak(Box::new(ReturnLoweringContext::new(
            custom_types,
            data_types,
        )));
        let callback_registry = Box::leak(Box::new(CallbackTraitRegistry::default()));
        let on_wire_record_error = Box::leak(Box::new(proc_macro2::TokenStream::new()));
        AsyncParamLowerer::new(return_lowering, callback_registry, on_wire_record_error)
    }

    #[test]
    fn rejects_async_callback_param() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(callback: impl Fn(i32) -> i32) {}
        };

        let error = async_param_lowerer()
            .validate_inputs(&function.sig.inputs)
            .expect_err("expected rejection");
        assert!(
            error
                .to_string()
                .contains("do not support closure callback parameters yet")
        );
    }

    #[test]
    fn rejects_async_mutable_slice_param() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(values: &mut [i32]) {}
        };

        let error = async_param_lowerer()
            .validate_inputs(&function.sig.inputs)
            .expect_err("expected rejection");
        assert!(
            error
                .to_string()
                .contains("do not support mutable slice parameters")
        );
    }

    #[test]
    fn rejects_async_trait_object_params() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(
                boxed: Box<dyn ExampleTrait>,
                shared: std::sync::Arc<dyn ExampleTrait>,
                optional: Option<std::sync::Arc<dyn ExampleTrait>>
            ) {}
        };

        let error = async_param_lowerer()
            .validate_inputs(&function.sig.inputs)
            .expect_err("expected rejection");
        assert!(
            error
                .to_string()
                .contains("do not support trait object callback parameters")
        );
    }

    #[test]
    fn accepts_supported_async_params() {
        let function: syn::ItemFn = parse_quote! {
            async fn demo(name: String, label: &String, ids: Vec<i32>, scores: &[i32], id: i64) {}
        };

        assert!(
            async_param_lowerer()
                .validate_inputs(&function.sig.inputs)
                .is_ok()
        );
    }
}

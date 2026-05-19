pub(crate) mod classify;
pub(crate) mod lower;
pub(crate) mod model;

#[cfg(test)]
mod tests {
    use super::classify::classify_value_return_strategy;
    use super::model::{ResolvedReturn, WasmOptionScalarEncoding};
    use crate::index::custom_types::CustomTypeRegistry;
    use crate::index::data_types::DataTypeRegistry;
    use crate::lowering::returns::model::ReturnLoweringContext;
    use boltffi_ffi_rules::transport::{
        EncodedReturnStrategy, ReturnContract, ReturnInvocationContext, ReturnPlatform,
        ValueReturnMethod, ValueReturnStrategy,
    };
    use syn::parse_quote;

    fn empty_return_lowering_context<'a>(
        custom_types: &'a CustomTypeRegistry,
        data_types: &'a DataTypeRegistry,
    ) -> ReturnLoweringContext<'a> {
        ReturnLoweringContext::new(custom_types, data_types)
    }

    #[test]
    fn wasm_option_bool_uses_numeric_bool_encoding() {
        let value_ident = syn::Ident::new("value", proc_macro2::Span::call_site());
        let expression =
            WasmOptionScalarEncoding::from_option_rust_type(&parse_quote!(Option<bool>))
                .expect("expected bool option encoding")
                .some_expression(&value_ident)
                .to_string();

        assert_eq!(expression, "if value { 1.0 } else { 0.0 }");
    }

    #[test]
    fn wasm_option_i64_is_not_nan_boxed() {
        assert!(
            WasmOptionScalarEncoding::from_option_rust_type(&parse_quote!(Option<i64>)).is_none()
        );
        assert!(
            WasmOptionScalarEncoding::from_option_rust_type(&parse_quote!(Option<u64>)).is_none()
        );
    }

    #[test]
    fn option_i64_and_u64_return_use_wire_encoding_to_preserve_bigint_payloads() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let context = empty_return_lowering_context(&custom_types, &data_types);

        let i64_strategy = classify_value_return_strategy(&parse_quote!(Option<i64>), &context);
        let u64_strategy = classify_value_return_strategy(&parse_quote!(Option<u64>), &context);

        assert_eq!(
            i64_strategy,
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        );
        assert_eq!(
            u64_strategy,
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        );
    }

    #[test]
    fn option_i32_return_keeps_compact_scalar_encoding() {
        let custom_types = CustomTypeRegistry::default();
        let data_types = DataTypeRegistry::default();
        let context = empty_return_lowering_context(&custom_types, &data_types);

        let strategy = classify_value_return_strategy(&parse_quote!(Option<i32>), &context);

        assert_eq!(
            strategy,
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::OptionScalar)
        );
    }

    #[test]
    fn packed_encoded_return_uses_packed_default_on_wasm_failure() {
        let resolved_return = ResolvedReturn::new(
            parse_quote!(std::time::Duration),
            ReturnContract::infallible(ValueReturnStrategy::Buffer(
                EncodedReturnStrategy::WireEncoded,
            )),
        );

        let statement = resolved_return
            .invalid_arg_early_return_statement()
            .to_string();

        assert!(matches!(
            resolved_return.value_return_strategy(),
            ValueReturnStrategy::Buffer(EncodedReturnStrategy::WireEncoded)
        ));
        assert!(matches!(
            resolved_return
                .value_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm,),
            ValueReturnMethod::DirectReturn
        ));
        assert!(statement.contains("FfiBuf :: default () . into_packed ()"));
        assert!(statement.contains("return :: boltffi :: __private :: FfiBuf :: default ()"));
    }

    #[test]
    fn direct_vec_return_uses_platform_aware_early_return() {
        let resolved_return = ResolvedReturn::new(
            parse_quote!(Vec<i32>),
            ReturnContract::infallible(ValueReturnStrategy::Buffer(
                EncodedReturnStrategy::DirectVec,
            )),
        );

        assert!(matches!(
            resolved_return
                .value_return_method(ReturnInvocationContext::SyncExport, ReturnPlatform::Wasm,),
            ValueReturnMethod::WriteToReturnSlot
        ));

        let combined = resolved_return
            .invalid_arg_early_return_statement()
            .to_string();
        assert!(
            combined.contains("return ;"),
            "combined: wasm branch should use void return"
        );
        assert!(
            combined.contains("return :: boltffi :: __private :: FfiBuf :: default ()"),
            "combined: native branch should return FfiBuf::default()"
        );

        assert_eq!(
            resolved_return
                .wasm_invalid_arg_early_return_statement()
                .to_string(),
            "return ;",
        );
        assert_eq!(
            resolved_return
                .native_invalid_arg_early_return_statement()
                .to_string(),
            "return :: boltffi :: __private :: FfiBuf :: default () ;",
        );
    }
}

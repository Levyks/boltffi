use super::rendered_fixture;

#[test]
fn kotlin_target_renders_primitive_function_stack() {
    insta::assert_snapshot!(rendered_fixture("exports/primitive_functions"));
}

#[test]
fn kotlin_target_preserves_unsigned_public_api_and_native_carriers() {
    insta::assert_snapshot!(rendered_fixture("exports/unsigned_functions"));
}

#[test]
fn kotlin_target_renders_string_functions_as_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("exports/string_functions"));
}

#[test]
fn kotlin_target_renders_direct_records_and_function_bridges() {
    insta::assert_snapshot!(rendered_fixture("exports/direct_records_and_c_style_enums"));
}

#[test]
fn kotlin_target_renders_encoded_records_through_codec_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/encoded_record_functions"));
}

#[test]
fn kotlin_target_renders_data_enums_through_codec_methods() {
    insta::assert_snapshot!(rendered_fixture("exports/encoded_functions"));
}

#[test]
fn kotlin_target_renders_fallible_returns_as_throwing_functions() {
    insta::assert_snapshot!(rendered_fixture("exports/fallible_returns"));
}

#[test]
fn kotlin_target_renders_custom_types_through_representations() {
    insta::assert_snapshot!(rendered_fixture("exports/custom_type_functions"));
}

#[test]
fn kotlin_target_renders_result_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/result_values"));
}

#[test]
fn kotlin_target_renders_builtin_values_through_shared_codec() {
    insta::assert_snapshot!(rendered_fixture("exports/builtin_functions"));
}

#[test]
fn kotlin_target_encodes_nullable_primitives_as_compact_wire() {
    insta::assert_snapshot!(rendered_fixture("exports/nullable_primitive_functions"));
}

#[test]
fn kotlin_target_renders_class_handles_and_associated_callables() {
    insta::assert_snapshot!(rendered_fixture("exports/kotlin_class_handles"));
}

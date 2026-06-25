use super::rendered_fixture;

#[test]
fn kotlin_target_renders_callback_handle_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/foreign_callback_parameter"));
}

#[test]
fn kotlin_target_renders_callback_enum_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_enum_parameter"));
}

#[test]
fn kotlin_target_renders_callback_encoded_payloads() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_encoded_return"));
}

#[test]
fn kotlin_target_renders_callback_encoded_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_byte_slice_parameter"));
}

#[test]
fn kotlin_target_renders_callback_record_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_record_parameter"));
}

#[test]
fn kotlin_target_renders_callback_vectors() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_direct_vector_parameter"
    ));
}

#[test]
fn kotlin_target_renders_callback_optional_scalar_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_optional_scalar_return"));
}

#[test]
fn kotlin_target_renders_callback_result_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_status_result"));
}

#[test]
fn kotlin_target_renders_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_handle_return"));
}

#[test]
fn kotlin_target_renders_nullable_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/nullable_callback_handle_return"));
}

#[test]
fn kotlin_target_renders_async_callback_return_shapes() {
    insta::assert_snapshot!(rendered_fixture("callback/async_callback_return_shapes"));
}

#[test]
fn kotlin_target_renders_async_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_returning_callback_handle"
    ));
}

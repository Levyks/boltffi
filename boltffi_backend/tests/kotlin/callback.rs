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
    let rendered = rendered_fixture("callback/callback_status_result");

    assert!(rendered.contains("catch (__boltffi_mapStatus_error: Throwable)"));
    assert!(!rendered.contains("catch (__boltffi_mapStatus_error: String)"));
    assert!(rendered.contains("val __boltffi_mapStatus_result = impl.mapStatus(status)"));
    assert_eq!(rendered.matches("impl.mapStatus(status)").count(), 1);

    insta::assert_snapshot!(rendered);
}

#[test]
fn kotlin_target_renders_callback_encoded_result_returns() {
    let rendered = rendered_fixture("callback/callback_encoded_status_result");

    assert!(rendered.contains("catch (__boltffi_mapMessage_error: Throwable)"));
    assert!(!rendered.contains("catch (__boltffi_mapMessage_error: String)"));
    assert!(rendered.contains("val __boltffi_mapMessage_result = impl.mapMessage(key)"));
    assert_eq!(rendered.matches("impl.mapMessage(key)").count(), 1);

    insta::assert_snapshot!(rendered);
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

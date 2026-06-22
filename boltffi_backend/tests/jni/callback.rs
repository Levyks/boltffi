use super::rendered_fixture;

#[test]
fn jni_bridge_renders_callback_handle_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/foreign_callback_parameter"));
}

#[test]
fn jni_bridge_caches_android_callback_threads_with_local_frames() {
    insta::assert_snapshot!(rendered_fixture("callback/foreign_callback_parameter"));
}

#[test]
fn jni_bridge_renders_callback_byte_slice_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_byte_slice_parameter"));
}

#[test]
fn jni_bridge_renders_callback_handle_method_parameters() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_method_callback_handle_parameter"
    ));
}

#[test]
fn jni_bridge_renders_callback_record_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_record_parameter"));
}

#[test]
fn jni_bridge_renders_callback_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_closure_parameter"));
}

#[test]
fn jni_bridge_renders_callback_encoded_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_encoded_closure_parameter"
    ));
}

#[test]
fn jni_bridge_renders_callback_direct_vector_closure_parameters() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_direct_vector_closure_parameter"
    ));
}

#[test]
fn jni_bridge_renders_callback_closure_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_closure_return"));
}

#[test]
fn jni_bridge_renders_callback_handle_method_closure_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/returned_callback_closure_return"
    ));
}

#[test]
fn jni_bridge_renders_callback_encoded_closure_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_encoded_closure_return"));
}

#[test]
fn jni_bridge_renders_callback_encoded_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_encoded_return"));
}

#[test]
fn jni_bridge_keeps_callback_status_param_separate_from_generated_status() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_status_result"));
}

#[test]
fn jni_bridge_renders_callback_record_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_record_return"));
}

#[test]
fn jni_bridge_renders_async_callback_completions() {
    insta::assert_snapshot!(rendered_fixture("callback/async_callback_string_return"));
}

#[test]
fn jni_bridge_renders_async_callback_completion_shapes() {
    insta::assert_snapshot!(rendered_fixture("callback/async_callback_return_shapes"));
}

#[test]
fn jni_bridge_renders_c_style_enum_async_callback_completion_payloads() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_c_style_enum_result"
    ));
}

#[test]
fn jni_bridge_renders_async_callback_handle_completion_payloads() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/async_callback_returning_callback_handle"
    ));
}

#[test]
fn jni_bridge_renders_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/callback_handle_return"));
}

#[test]
fn jni_bridge_renders_async_callback_handle_methods() {
    insta::assert_snapshot!(rendered_fixture("callback/returned_async_callback_handle"));
}

#[test]
fn jni_bridge_renders_async_callback_handle_method_payloads() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/returned_async_callback_return_shapes"
    ));
}

#[test]
fn jni_bridge_renders_callback_method_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture(
        "callback/callback_method_callback_handle_return"
    ));
}

#[test]
fn jni_bridge_renders_nullable_callback_handle_returns() {
    insta::assert_snapshot!(rendered_fixture("callback/nullable_callback_handle_return"));
}

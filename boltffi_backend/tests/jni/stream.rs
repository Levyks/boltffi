use super::rendered_fixture;

#[test]
fn jni_bridge_renders_stream_protocol_functions() {
    insta::assert_snapshot!(rendered_fixture("stream/protocol_functions"));
}

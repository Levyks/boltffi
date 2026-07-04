use super::rendered_fixture;

#[test]
fn kotlin_target_renders_stream_protocols() {
    insta::assert_snapshot!(rendered_fixture("stream/protocol_functions"));
}

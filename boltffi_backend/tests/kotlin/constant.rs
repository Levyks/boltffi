use super::rendered_fixture;

#[test]
fn kotlin_target_renders_constants() {
    insta::assert_snapshot!(rendered_fixture("constant/literals_and_accessors"));
}

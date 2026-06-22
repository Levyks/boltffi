use super::rendered_fixture;

#[test]
fn jni_bridge_maps_primitive_direct_vectors_to_java_primitive_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/primitive_vector_parameter"));
}

#[test]
fn jni_bridge_maps_direct_record_vectors_to_java_byte_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/record_vector_parameter"));
}

#[test]
fn jni_bridge_maps_callback_direct_vectors_to_java_primitive_arrays() {
    insta::assert_snapshot!(rendered_fixture("direct_vector/callback_vector_parameter"));
}

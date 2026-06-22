use super::files;

#[test]
fn jni_bridge_renders_callback_method_shared_callback_handle_returns() {
    let files = files(
        r#"
            use std::sync::Arc;

            #[export]
            pub trait Child {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub trait Listener {
                fn child(&self) -> Arc<dyn Child>;
            }
            "#,
    );
    let header = files
        .iter()
        .find(|(path, _)| path == "jni/demo.h")
        .map(|(_, contents)| contents)
        .expect("C header file");
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    assert!(header.contains("BoltFFICallbackHandle (*child)(uint64_t);"));
    [
        "GetStaticMethodID(env, g____ListenerVTable_class, \"child\", \"(J)J\")",
        "jlong __boltffi_return_handle = (*env)->CallStaticLongMethod(env, g____ListenerVTable_class, g____ListenerVTable_child_method",
        "BoltFFICallbackHandle result = boltffi_create_callback_demo_child((uint64_t)__boltffi_return_handle);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
}

#[test]
fn jni_bridge_renders_callback_method_nullable_callback_handle_returns() {
    let files = files(
        r#"
            use std::sync::Arc;

            #[export]
            pub trait Child {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub trait Listener {
                fn optional_boxed_child(&self) -> Option<Box<dyn Child>>;
                fn optional_shared_child(&self) -> Option<Arc<dyn Child>>;
            }
            "#,
    );
    let header = files
        .iter()
        .find(|(path, _)| path == "jni/demo.h")
        .map(|(_, contents)| contents)
        .expect("C header file");
    let source = files
        .iter()
        .find(|(path, _)| path == "jni/jni_glue.c")
        .map(|(_, contents)| contents)
        .expect("JNI source file");

    [
        "BoltFFICallbackHandle (*optional_boxed_child)(uint64_t);",
        "BoltFFICallbackHandle (*optional_shared_child)(uint64_t);",
    ]
    .into_iter()
    .for_each(|expected| assert!(header.contains(expected), "{expected}"));

    [
        "GetStaticMethodID(env, g____ListenerVTable_class, \"optional_boxed_child\", \"(J)J\")",
        "GetStaticMethodID(env, g____ListenerVTable_class, \"optional_shared_child\", \"(J)J\")",
        "BoltFFICallbackHandle result = boltffi_create_callback_demo_child((uint64_t)__boltffi_return_handle);",
    ]
    .into_iter()
    .for_each(|expected| assert!(source.contains(expected), "{expected}"));
}

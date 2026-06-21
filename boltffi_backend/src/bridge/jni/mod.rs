//! JNI bridge.
//!
//! This bridge layers above the C ABI bridge. It emits C functions with
//! JNI-exported names and gives JVM hosts a typed native-method contract.

mod bridge;
mod contract;
mod name;
mod template;

pub use bridge::JniBridge;
pub use contract::{JniBridgeContract, JniType, NativeMethod, NativeParameter, NativeReturn};
pub use name::{JniSymbolName, JvmClassPath, JvmNameSegment};

#[cfg(test)]
mod tests {
    use std::path::Path;

    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Native, lower};

    use crate::{
        bridge::{
            c::CBridge,
            jni::{JniBridge, JniBridgeContract},
        },
        core::{BridgeLayer, BridgeOutput, BridgeStack},
    };

    fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
        let file = syn::parse_str(source).expect("valid source fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("fixture should scan");
        lower::<Native>(&source).expect("fixture should lower")
    }

    fn bridge(source: &str) -> BridgeOutput<JniBridgeContract> {
        let bindings = bindings(source);
        let stack = BridgeLayer::new(
            CBridge::new("jni/demo.h").expect("C header bridge"),
            JniBridge::new("com.boltffi.demo", "Native", "jni/jni_glue.c").expect("JNI bridge"),
        );
        stack.build(&bindings).expect("JNI bridge stack")
    }

    fn files(source: &str) -> Vec<(String, String)> {
        bridge(source)
            .output()
            .files()
            .iter()
            .map(|file| {
                (
                    file.path().as_path().display().to_string(),
                    file.contents().to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn jni_bridge_layers_primitive_functions_on_c_bridge() {
        let files = files(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }

            #[export]
            pub fn enabled(flag: bool) -> bool {
                flag
            }

            #[export]
            pub fn refresh() {}
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

        assert!(header.contains("int32_t boltffi_function_demo_add(int32_t left, int32_t right);"));
        assert!(source.contains("#include \"demo.h\""));
        assert!(source.contains("JNIEXPORT jint JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1add(JNIEnv *env, jclass cls, jint left, jint right)"));
        assert!(source.contains("jint result = boltffi_function_demo_add(left, right);"));
        assert!(source.contains("JNIEXPORT jboolean JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1enabled(JNIEnv *env, jclass cls, jboolean flag)"));
        assert!(source.contains("return (jboolean)result;"));
        assert!(source.contains("JNIEXPORT void JNICALL Java_com_boltffi_demo_Native_boltffi_1function_1demo_1refresh(JNIEnv *env, jclass cls)"));
        assert!(source.contains("FfiStatus status = boltffi_function_demo_refresh();"));
        assert!(source.contains("boltffi_jni_throw_status(env, status);"));
    }

    #[test]
    fn jni_bridge_contract_records_class_and_source_path() {
        let output = bridge(
            r#"
            #[export]
            pub fn add(left: i32, right: i32) -> i32 {
                left + right
            }
            "#,
        );
        let contract = output.contract();

        assert_eq!(contract.class().as_java_path(), "com.boltffi.demo.Native");
        assert_eq!(
            contract.source_path().as_path(),
            Path::new("jni/jni_glue.c")
        );
        assert_eq!(contract.c_header().as_str(), "demo.h");
        assert_eq!(contract.methods().len(), 1);
        assert_eq!(
            contract.methods()[0].symbol().to_string(),
            "Java_com_boltffi_demo_Native_boltffi_1function_1demo_1add"
        );
    }
}

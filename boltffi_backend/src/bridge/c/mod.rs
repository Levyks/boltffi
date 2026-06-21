//! C ABI bridge.
//!
//! This bridge turns native BoltFFI bindings into a single C header. The
//! contract is still typed Rust data, so host renderers can inspect the C ABI
//! without parsing header text.

mod contract;
mod header;
mod identifier;
mod name;
pub(crate) mod syntax;
mod template;

pub use contract::{
    CBridgeContract, Callback, Enum, EnumVariant, Field, Function, Parameter, Record,
    SupportFunctions, Type,
};
pub use header::{CBridge, HeaderInclude};
pub use identifier::Identifier;
pub use syntax::{ArgumentList, Expression, Literal, Statement, Syntax, TypeFragment};

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Native, lower};

    use crate::core::bridge::BridgeBackend;

    use super::CBridge;

    fn bindings(source: &str) -> boltffi_binding::Bindings<Native> {
        let file = syn::parse_str(source).expect("valid source fixture");
        let source = boltffi_scan::scan_file(file, PackageInfo::new("demo", None))
            .expect("fixture should scan");
        lower::<Native>(&source).expect("fixture should lower")
    }

    fn header(source: &str) -> String {
        let bindings = bindings(source);
        let bridge = CBridge::default_header().expect("header bridge");
        let contract = bridge.build_contract(&bindings).expect("C bridge contract");
        let output = bridge
            .render_bridge(&bindings, &contract)
            .expect("C header render");
        let files = output.files();
        assert_eq!(files.len(), 1);
        files[0].contents().to_owned()
    }

    #[test]
    fn c_header_renders_bindings_declaration_surface() {
        let header = header(
            r#"
            use std::sync::Arc;
            use boltffi::EventSubscription;

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[data]
            pub struct Person {
                pub name: String,
            }

            #[repr(u8)]
            #[data]
            pub enum Mode {
                Fast = 1,
                Slow = 2,
            }

            #[data]
            pub enum Shape {
                Dot(Point),
                Label(String),
            }

            custom_type!(
                pub Timestamp,
                remote = TimestampRust,
                repr = i64,
                into_ffi = timestamp_into_ffi,
                try_from_ffi = timestamp_from_ffi
            );

            #[export]
            pub trait Listener {
                fn notify(&self, code: u32);
                fn on_value(&self, value: u32) -> i64;
                async fn load(&self, key: u32) -> String;
                async fn locate(&self, point: Point) -> Point;
                async fn maybe_value(&self, key: u32) -> Option<i64>;
                async fn values(&self, count: u32) -> Vec<u32>;
                async fn try_load(&self, key: u32) -> Result<String, Mode>;
            }

            pub struct Engine;

            #[export(single_threaded)]
            impl Engine {
                pub fn new(seed: u64) -> Self { todo!() }
                pub fn version() -> u32 { 1 }
                pub fn score(&self, point: Point) -> u32 { 0 }
                pub fn advance(&mut self, delta: u32) {}

                #[ffi_stream(item = Point, mode = "batch")]
                pub fn points(&self) -> Arc<EventSubscription<Point>> { todo!() }

                #[ffi_stream(item = String)]
                pub fn names(&self) -> Arc<EventSubscription<String>> { todo!() }
            }

            #[data(impl)]
            impl Point {
                pub fn origin() -> Self { todo!() }
                pub fn distance(&self, other: Point) -> f64 { todo!() }
            }

            #[data(impl)]
            impl Person {
                pub fn rename(&self, name: String) -> String { name }
            }

            #[data(impl)]
            impl Mode {
                pub fn default() -> Self { todo!() }
                pub fn code(&self) -> u8 { 0 }
            }

            #[export]
            pub fn add(left: i32, right: i32) -> i32 { left + right }

            #[export]
            pub async fn fetch_count() -> u32 { 7 }

            #[export]
            pub async fn refresh() {}

            #[export]
            pub fn greet(name: String) -> String { name }

            #[export]
            pub fn keep_shape(shape: Shape) -> Shape { shape }

            #[export]
            pub fn remember(time: TimestampRust) -> TimestampRust { time }

            #[export]
            pub fn shift(offset: isize) -> isize { offset }

            #[export]
            pub fn install(listener: impl Listener, callback: impl Fn(u32) -> u32) {}

            #[export]
            pub fn install_void(callback: impl Fn(u32)) {}

            #[export]
            pub const ANSWER: u32 = 42;

            #[export]
            pub const MAGIC: &'static [u8] = b"ffi";
            "#,
        );
        let golden = include_str!("../../../fixtures/c_bridge_declaration_surface.h");
        assert_eq!(header.trim_end(), golden.trim_end());
    }

    #[test]
    fn c_header_renders_async_callback_completion_payloads() {
        let header = header(
            r#"
            use boltffi::*;

            #[repr(C)]
            #[data]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }

            #[repr(i32)]
            #[data]
            pub enum MathError {
                Bad = 1,
            }

            #[export]
            #[allow(async_fn_in_trait)]
            pub trait AsyncCallbacks: Send + Sync {
                async fn value(&self, key: i32) -> i32;
                async fn point(&self, point: Point) -> Point;
                async fn maybe(&self, key: i32) -> Option<i64>;
                async fn numbers(&self, count: i32) -> Vec<i32>;
                async fn fallible(&self, key: i32) -> Result<String, MathError>;
            }
            "#,
        );
        [
            "void (*value)(uint64_t, int32_t, void (*)(void *, FfiStatus, int32_t), void *);",
            "void (*point)(uint64_t, ___Point, void (*)(void *, FfiStatus, ___Point), void *);",
            "void (*maybe)(uint64_t, int32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);",
            "void (*numbers)(uint64_t, int32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);",
            "void (*fallible)(uint64_t, int32_t, void (*)(void *, FfiStatus, FfiBuf_u8), void *);",
        ]
        .into_iter()
        .for_each(|signature| assert!(header.contains(signature), "{signature}\n{header}"));
    }
}

//! C ABI bridge.
//!
//! This bridge turns native BoltFFI bindings into a single C header. The
//! contract is still typed Rust data, so host renderers can inspect the C ABI
//! without parsing header text.

mod callback;
mod contract;
mod enumeration;
mod function;
mod header;
mod identifier;
mod name;
mod names;
mod parameter;
mod record;
mod support;
pub(crate) mod syntax;
mod template;
mod ty;

pub use callback::{Callback, CallbackSlot};
pub use contract::CBridgeContract;
pub use enumeration::{Enum, EnumVariant};
pub use function::Function;
pub use header::{CBridge, HeaderInclude};
pub use identifier::Identifier;
pub use parameter::{
    ByteSliceParameter, CallbackCompletionParameter, ClosureParameter, ContinuationParameter,
    Parameter, ParameterGroup, ParameterIndex,
};
pub use record::{Field, Record};
pub use support::SupportFunctions;
pub use syntax::{ArgumentList, Expression, Literal, Statement, Syntax, TypeFragment};
pub use ty::Type;

const C_BRIDGE_LAYER: &str = "c bridge";
const C_BRIDGE_CONTRACT: &str = "c";

#[cfg(test)]
mod tests {
    use boltffi_ast::PackageInfo;
    use boltffi_binding::{Native, lower};

    use crate::core::bridge::BridgeBackend;

    use super::{CBridge, ParameterGroup, Type};

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

    fn contract(source: &str) -> super::CBridgeContract {
        let bindings = bindings(source);
        let bridge = CBridge::default_header().expect("header bridge");
        bridge.build_contract(&bindings).expect("C bridge contract")
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

    #[test]
    fn c_contract_groups_closure_parameter_triples() {
        let contract = contract(
            r#"
            #[export]
            pub fn install(callback: impl Fn(u32) -> u32) {}
            "#,
        );
        let function = contract
            .functions()
            .iter()
            .find(|function| function.name() == "boltffi_function_demo_install")
            .expect("exported function");
        let [ParameterGroup::Closure(closure)] = function.parameter_groups() else {
            panic!("expected one closure parameter group");
        };

        assert_eq!(closure.name(), "callback");
        assert_eq!(closure.signature().as_str(), "U32ToU32");
        assert_eq!(function.parameter(closure.call()).name(), "callback_call");
        assert_eq!(
            function.parameter(closure.context()).name(),
            "callback_context"
        );
        assert_eq!(
            function.parameter(closure.release()).name(),
            "callback_release"
        );
    }

    #[test]
    fn c_contract_groups_callback_method_closure_parameter_triples() {
        let contract = contract(
            r#"
            #[export]
            pub trait Listener {
                fn on_event(&self, callback: impl Fn(u32) -> u32);
            }
            "#,
        );
        let callback = contract
            .callbacks()
            .iter()
            .find(|callback| {
                callback.create_handle().name() == "boltffi_create_callback_demo_listener"
            })
            .expect("callback declaration");
        let slot = callback
            .methods()
            .iter()
            .find(|slot| slot.name().as_str() == "on_event")
            .expect("callback method slot");
        let [
            ParameterGroup::Value(handle),
            ParameterGroup::Closure(closure),
        ] = slot.parameter_groups()
        else {
            panic!("expected handle plus closure parameter group");
        };

        assert_eq!(slot.parameter(*handle).name(), "handle");
        assert_eq!(closure.name(), "callback");
        assert_eq!(closure.signature().as_str(), "U32ToU32");
        assert_eq!(slot.parameter(closure.call()).name(), "callback_call");
        assert_eq!(slot.parameter(closure.context()).name(), "callback_context");
        assert_eq!(slot.parameter(closure.release()).name(), "callback_release");
    }

    #[test]
    fn c_contract_preserves_callback_handle_identity() {
        let contract = contract(
            r#"
            #[export]
            pub trait Listener {
                fn on_value(&self, value: u32) -> u32;
            }

            #[export]
            pub fn install(listener: impl Listener) {}
            "#,
        );
        let callback = contract
            .callbacks()
            .iter()
            .find(|callback| {
                callback.create_handle().name() == "boltffi_create_callback_demo_listener"
            })
            .expect("callback declaration");
        let function = contract
            .functions()
            .iter()
            .find(|function| function.name() == "boltffi_function_demo_install")
            .expect("exported function");
        let [ParameterGroup::Value(listener)] = function.parameter_groups() else {
            panic!("expected one callback-handle parameter");
        };

        assert_eq!(
            function.parameter(*listener).ty(),
            &Type::CallbackHandle(callback.id())
        );
        assert_eq!(
            callback.create_handle().returns(),
            &Type::CallbackHandle(callback.id())
        );
    }

    #[test]
    fn c_contract_groups_encoded_byte_slice_parameters() {
        let contract = contract(
            r#"
            #[export]
            pub fn greet(name: String) -> String {
                name
            }
            "#,
        );
        let function = contract
            .functions()
            .iter()
            .find(|function| function.name() == "boltffi_function_demo_greet")
            .expect("exported function");
        let [ParameterGroup::ByteSlice(bytes)] = function.parameter_groups() else {
            panic!("expected one byte-slice parameter group");
        };

        assert_eq!(bytes.name(), "name");
        assert_eq!(function.parameter(bytes.pointer()).name(), "name_ptr");
        assert_eq!(function.parameter(bytes.length()).name(), "name_len");
    }

    #[test]
    fn c_contract_groups_encoded_receiver_byte_slice_parameters() {
        let contract = contract(
            r#"
            #[data]
            pub struct Person {
                pub name: String,
            }

            #[data(impl)]
            impl Person {
                pub fn rename(&self, name: String) -> String {
                    name
                }
            }
            "#,
        );
        let function = contract
            .functions()
            .iter()
            .find(|function| function.name() == "boltffi_method_record_demo_person_rename")
            .expect("encoded record method");
        let [
            ParameterGroup::ByteSlice(receiver),
            ParameterGroup::ByteSlice(name),
        ] = function.parameter_groups()
        else {
            panic!("expected receiver and name byte-slice parameter groups");
        };

        assert_eq!(receiver.name(), "receiver");
        assert_eq!(
            function.parameter(receiver.pointer()).name(),
            "receiver_ptr"
        );
        assert_eq!(function.parameter(receiver.length()).name(), "receiver_len");
        assert_eq!(name.name(), "name");
        assert_eq!(function.parameter(name.pointer()).name(), "name_ptr");
        assert_eq!(function.parameter(name.length()).name(), "name_len");
    }

    #[test]
    fn c_contract_groups_async_poll_continuations() {
        let contract = contract(
            r#"
            #[export]
            pub async fn fetch_count() -> u32 {
                7
            }
            "#,
        );
        let function = contract
            .functions()
            .iter()
            .find(|function| function.name() == "boltffi_async_function_demo_fetch_count_poll")
            .expect("async poll function");
        let [
            ParameterGroup::Value(_),
            ParameterGroup::Continuation(continuation),
        ] = function.parameter_groups()
        else {
            panic!("expected handle plus continuation parameter groups");
        };

        assert_eq!(continuation.name(), "callback");
        assert_eq!(
            function.parameter(continuation.data()).name(),
            "callback_data"
        );
        assert_eq!(
            function.parameter(continuation.callback()).name(),
            "callback"
        );
    }

    #[test]
    fn c_contract_groups_async_callback_completions() {
        let contract = contract(
            r#"
            #[export]
            pub trait Listener {
                async fn load(&self, key: u32) -> String;
            }
            "#,
        );
        let callback = contract
            .callbacks()
            .iter()
            .find(|callback| callback.vtable().name() == "___ListenerVTable")
            .expect("callback vtable");
        let method = callback
            .methods()
            .iter()
            .find(|method| method.name().as_str() == "load")
            .expect("async callback method");

        assert!(matches!(method.returns(), Type::Void));
        let [
            ParameterGroup::Value(_),
            ParameterGroup::Value(_),
            ParameterGroup::CallbackCompletion(completion),
        ] = method.parameter_groups()
        else {
            panic!("expected handle, key, and callback completion groups");
        };

        assert_eq!(completion.name(), "complete");
        assert_eq!(method.parameter(completion.callback()).name(), "complete");
        assert_eq!(
            method.parameter(completion.context()).name(),
            "complete_context"
        );
    }
}

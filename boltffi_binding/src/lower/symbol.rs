//! Native symbol minting and id allocation for the lowering pass.
//!
//! Every callable the lowered IR exposes references one or more native
//! symbols by id. Ids are sequential integers assigned in the order the
//! pass mints them. Names use separate lanes for user callables,
//! initializers, and runtime lifecycle functions so source members named
//! `free`, `release`, or `new` cannot collide with symbols the runtime
//! needs for ownership management.

use crate::{NativeSymbol, SymbolId, SymbolName};

use super::LowerError;

/// Symbol prefix shared by every binding the contract exposes.
pub const FFI_PREFIX: &str = "boltffi";

#[derive(Clone, Copy)]
pub enum SymbolOwner<'a> {
    Record(&'a str),
    Enum(&'a str),
    Class(&'a str),
    Callback(&'a str),
}

impl<'a> SymbolOwner<'a> {
    pub const fn record(source_id: &'a str) -> Self {
        Self::Record(source_id)
    }

    pub const fn enumeration(source_id: &'a str) -> Self {
        Self::Enum(source_id)
    }

    pub const fn class(source_id: &'a str) -> Self {
        Self::Class(source_id)
    }

    pub const fn callback(source_id: &'a str) -> Self {
        Self::Callback(source_id)
    }

    fn family(self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::Enum(_) => "enum",
            Self::Class(_) => "class",
            Self::Callback(_) => "callback",
        }
    }

    fn source_id(self) -> &'a str {
        match self {
            Self::Record(source_id)
            | Self::Enum(source_id)
            | Self::Class(source_id)
            | Self::Callback(source_id) => source_id,
        }
    }
}

/// Hands out [`SymbolId`]s in the order callers mint native symbols.
///
/// Ids are stable inside one [`crate::Bindings`](crate::Bindings) value
/// but carry no meaning outside it; their job is to keep equal symbols
/// equal across the contract's symbol table.
pub struct SymbolAllocator {
    next: u32,
}

impl SymbolAllocator {
    pub fn new() -> Self {
        Self { next: 0 }
    }

    /// Mints a [`NativeSymbol`] from a constructed FFI name, allocating
    /// a fresh [`SymbolId`].
    pub fn mint(&mut self, name: String) -> Result<NativeSymbol, LowerError> {
        let id = self.next_id();
        let parsed = SymbolName::parse(name)?;
        Ok(NativeSymbol::new(id, parsed))
    }

    pub const fn next_group_id(&self) -> u32 {
        self.next
    }

    fn next_id(&mut self) -> SymbolId {
        let id = SymbolId::from_raw(self.next);
        self.next += 1;
        id
    }
}

/// Builds the symbol used for a named method owned by `owner`.
pub fn member_symbol_name(owner: SymbolOwner<'_>, member_name: &str) -> String {
    format!(
        "{}_method_{}_{}_{}",
        FFI_PREFIX,
        owner.family(),
        symbol_path(owner.source_id()),
        member_name
    )
}

/// Builds the symbol used for an initializer owned by `owner`.
pub fn initializer_symbol_name(owner: SymbolOwner<'_>, initializer_name: &str) -> String {
    format!(
        "{}_init_{}_{}_{}",
        FFI_PREFIX,
        owner.family(),
        symbol_path(owner.source_id()),
        initializer_name
    )
}

/// Builds the symbol used to drop a class handle on the Rust side.
pub fn class_release_symbol_name(class_id: &str) -> String {
    format!("{}_release_class_{}", FFI_PREFIX, symbol_path(class_id))
}

/// Builds the symbol foreign code links to invoke a free function.
///
/// Free functions have no owning type, so the symbol carries only the
/// `function` lane and the path. The path is the source id snake-cased,
/// matching the convention every other lane uses.
pub fn function_symbol_name(function_id: &str) -> String {
    format!("{}_function_{}", FFI_PREFIX, symbol_path(function_id))
}

/// Builds the symbol foreign code links to read a constant whose value
/// is delivered through an accessor rather than an inline literal.
///
/// Constants have no owning type, so the symbol carries only the `const`
/// lane and the source id snake-cased, matching the convention the
/// free-function lane uses.
pub fn constant_accessor_symbol_name(constant_id: &str) -> String {
    format!("{}_const_{}", FFI_PREFIX, symbol_path(constant_id))
}

/// Builds the Rust-side symbol that installs a foreign-provided vtable.
pub fn callback_register_symbol_name(callback_id: &str) -> String {
    format!(
        "{}_register_callback_{}",
        FFI_PREFIX,
        symbol_path(callback_id)
    )
}

/// Builds the Rust-side symbol that mints a callback handle bound to a
/// foreign implementation.
pub fn callback_create_handle_symbol_name(callback_id: &str) -> String {
    format!(
        "{}_create_callback_{}",
        FFI_PREFIX,
        symbol_path(callback_id)
    )
}

pub fn callback_local_handle_name(callback_name: &str) -> String {
    format!(
        "__{}_local_{}_handle",
        FFI_PREFIX,
        to_snake_case(callback_name)
    )
}

/// Builds the wasm import name foreign code provides for one method.
///
/// Takes a [`CallbackSlot`], so the value is guaranteed to be the
/// canonical snake-cased slot name. The native vtable slot and the
/// wasm import suffix for the same method are byte-equal by
/// construction; there is no `&str` precondition for a caller to
/// remember or violate.
pub fn callback_wasm_import_method_name(callback_id: &str, slot: &CallbackSlot) -> String {
    wasm_callback_import_name("method", &symbol_path(callback_id), slot.as_str())
}

pub fn callback_wasm_import_start_name(callback_id: &str, slot: &CallbackSlot) -> String {
    wasm_callback_import_name("async_start", &symbol_path(callback_id), slot.as_str())
}

pub fn callback_wasm_complete_symbol_name(callback_id: &str, slot: &CallbackSlot) -> String {
    format!(
        "{}_callback_{}_{}_complete",
        FFI_PREFIX,
        symbol_path(callback_id),
        slot.as_str()
    )
}

/// Builds the wasm import name foreign code provides to drop a handle.
pub fn callback_wasm_import_free_name(callback_id: &str) -> String {
    wasm_callback_import_name("lifecycle", &symbol_path(callback_id), "free")
}

/// Builds the wasm import name foreign code provides to duplicate a handle.
pub fn callback_wasm_import_clone_name(callback_id: &str) -> String {
    wasm_callback_import_name("lifecycle", &symbol_path(callback_id), "clone")
}

/// Names one symbol in the consumer-side stream protocol.
///
/// Every stream the contract exposes mints one symbol per action below.
/// The action suffix is appended to the stream's snake-cased canonical
/// id so the six symbols attached to one stream group together when
/// grepped: a stream `demo::events` mints
/// `boltffi_stream_demo_events_subscribe`, `..._pop_batch`, and so on.
#[derive(Clone, Copy)]
pub enum StreamLifecycle {
    /// Opens a subscription and returns the session handle.
    Subscribe,
    /// Drains a batch of buffered items into the foreign side.
    PopBatch,
    /// Blocks the foreign caller until at least one item is ready.
    Wait,
    /// Reports readiness without blocking.
    Poll,
    /// Closes a subscription.
    Unsubscribe,
    /// Drops the stream itself.
    Free,
}

impl StreamLifecycle {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Subscribe => "subscribe",
            Self::PopBatch => "pop_batch",
            Self::Wait => "wait",
            Self::Poll => "poll",
            Self::Unsubscribe => "unsubscribe",
            Self::Free => "free",
        }
    }
}

/// Builds the symbol foreign code links to invoke one stream-protocol
/// action.
///
/// `stream_id` is the canonical Rust path of the source declaration.
/// Class-owned streams already carry the class path in their id, so the
/// resulting symbol distinguishes them from standalone streams without
/// a separate lane.
pub fn stream_symbol_name(stream_id: &str, action: StreamLifecycle) -> String {
    format!(
        "{}_stream_{}_{}",
        FFI_PREFIX,
        symbol_path(stream_id),
        action.suffix()
    )
}

/// Names one symbol in the async lifecycle protocol of a single callable.
///
/// The lifecycle symbols share the start callable's symbol name as a
/// prefix in the async lane so every symbol attached to one async
/// operation groups when grepped or sorted without sharing the user's
/// callable namespace: a method `compute` on `demo::Engine` mints
/// `boltffi_async_method_record_demo_engine_compute_poll`,
/// `..._complete`, and so on.
#[derive(Clone, Copy)]
pub enum AsyncLifecycle {
    /// Foreign-side step that advances the async state without blocking.
    Poll,
    /// Wasm-side step that advances the async state synchronously.
    PollSync,
    /// Foreign-side step that extracts the resolved value once ready.
    Complete,
    /// Foreign-side step that requests cancellation.
    Cancel,
    /// Foreign-side step that releases the async state.
    Free,
    /// Foreign-side step that retrieves the panic message after a
    /// failed operation.
    Panic,
}

impl AsyncLifecycle {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Poll => "poll",
            Self::PollSync => "poll_sync",
            Self::Complete => "complete",
            Self::Cancel => "cancel",
            Self::Free => "free",
            Self::Panic => "panic_message",
        }
    }
}

/// Builds a lifecycle symbol name from the start callable's symbol name.
pub fn async_lifecycle_symbol_name(start_symbol_name: &str, action: AsyncLifecycle) -> String {
    let start_without_prefix = start_symbol_name
        .strip_prefix(&format!("{FFI_PREFIX}_"))
        .unwrap_or(start_symbol_name);
    format!(
        "{}_async_{}_{}",
        FFI_PREFIX,
        start_without_prefix,
        action.suffix()
    )
}

/// The canonical snake-cased name of a callback method's dispatch slot.
///
/// Every surface (native vtable slot, wasm import suffix) builds its
/// dispatch identifier from this same string, so wrapping the value in
/// a private newtype removes the convention that callers must normalize
/// a raw method ident before reaching the per-surface constructor. The
/// only path to a [`CallbackSlot`] runs through
/// [`CallbackSlot::from_method_name`], which applies [`to_snake_case`]
/// once.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CallbackSlot(String);

impl CallbackSlot {
    /// Normalizes a raw source method ident into the canonical slot name.
    pub fn from_method_name(method_name: &str) -> Self {
        Self(to_snake_case(method_name))
    }

    /// Returns the canonical slot name.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Wasm import module foreign callback implementations are linked from.
pub const WASM_CALLBACK_IMPORT_MODULE: &str = "env";

/// Vtable slot the runtime fills with the foreign-provided free fn.
pub const VTABLE_FREE_SLOT_NAME: &str = "free";

/// Vtable slot the runtime fills with the foreign-provided clone fn.
pub const VTABLE_CLONE_SLOT_NAME: &str = "clone";

fn symbol_path(source_id: &str) -> String {
    source_id
        .split("::")
        .filter(|segment| !segment.is_empty())
        .map(to_snake_case)
        .collect::<Vec<_>>()
        .join("_")
}

pub fn wasm_callback_import_name(lane: &str, owner: &str, action: &str) -> String {
    format!("__{}_callback_{}_{}_{}", FFI_PREFIX, lane, owner, action)
}

pub fn wasm_closure_export_name(group_id: u32, signature: &str, action: &str) -> String {
    format!(
        "{}_closure_{}_{}_{}",
        FFI_PREFIX, group_id, signature, action
    )
}

/// Lowercases `name` and inserts an underscore at every word boundary.
///
/// Word boundaries are:
///
/// - A lowercase or digit followed by an uppercase character, e.g.
///   `MyRecord` → `my_record`.
/// - An uppercase character at the end of an acronym, identified by
///   the next character being lowercase, e.g. `HTTPHeader` →
///   `http_header`, `XMLParser` → `xml_parser`.
///
/// Pure runs of uppercase characters (`HTTP`) collapse to lowercase
/// without internal underscores. Strings that already use snake_case
/// pass through unchanged.
pub fn to_snake_case(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    let initial = String::with_capacity(name.len() + chars.len() / 2);
    chars
        .iter()
        .enumerate()
        .fold(initial, |mut result, (index, &character)| {
            if character.is_uppercase() && index > 0 {
                let previous = chars[index - 1];
                let next = chars.get(index + 1).copied();
                let previous_is_word = previous.is_lowercase() || previous.is_ascii_digit();
                let acronym_word_break = previous.is_uppercase()
                    && next.is_some_and(|character| character.is_lowercase());
                if previous_is_word || acronym_word_break {
                    result.push('_');
                }
            }
            result.extend(character.to_lowercase());
            result
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_lowercases_camel_words() {
        assert_eq!(to_snake_case("MyRecord"), "my_record");
        assert_eq!(to_snake_case("Point"), "point");
    }

    #[test]
    fn snake_case_breaks_acronyms_before_following_word() {
        assert_eq!(to_snake_case("HTTPHeader"), "http_header");
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("MyHTTPClient"), "my_http_client");
    }

    #[test]
    fn snake_case_collapses_pure_acronyms() {
        assert_eq!(to_snake_case("HTTP"), "http");
        assert_eq!(to_snake_case("URL"), "url");
    }

    #[test]
    fn snake_case_passes_through_lowercase() {
        assert_eq!(to_snake_case("point"), "point");
        assert_eq!(to_snake_case("my_record"), "my_record");
    }

    #[test]
    fn snake_case_treats_digit_then_upper_as_word_break() {
        assert_eq!(to_snake_case("Point2D"), "point2_d");
        assert_eq!(to_snake_case("Vector3"), "vector3");
    }

    #[test]
    fn callback_local_handle_name_uses_local_callback_namespace() {
        assert_eq!(
            callback_local_handle_name("ProgressListener"),
            "__boltffi_local_progress_listener_handle"
        );
    }

    #[test]
    fn member_symbol_name_uses_owner_and_member() {
        assert_eq!(
            member_symbol_name(SymbolOwner::record("demo::MyRecord"), "translate"),
            "boltffi_method_record_demo_my_record_translate"
        );
    }

    #[test]
    fn initializer_symbol_name_uses_initializer_lane() {
        assert_eq!(
            initializer_symbol_name(SymbolOwner::record("demo::Point"), "new"),
            "boltffi_init_record_demo_point_new"
        );
    }

    #[test]
    fn constant_accessor_symbol_name_uses_const_lane() {
        assert_eq!(
            constant_accessor_symbol_name("demo::MAGIC"),
            "boltffi_const_demo_magic"
        );
    }

    #[test]
    fn class_release_symbol_name_uses_release_lane() {
        assert_eq!(
            class_release_symbol_name("demo::Engine"),
            "boltffi_release_class_demo_engine"
        );
    }

    #[test]
    fn symbol_paths_include_source_namespaces() {
        assert_eq!(
            member_symbol_name(SymbolOwner::class("demo::nested::HTTPClient"), "fetch"),
            "boltffi_method_class_demo_nested_http_client_fetch"
        );
    }

    #[test]
    fn allocator_mints_fresh_ids() {
        let mut allocator = SymbolAllocator::new();
        let first = allocator
            .mint("boltffi_demo_one".to_owned())
            .expect("valid name");
        let second = allocator
            .mint("boltffi_demo_two".to_owned())
            .expect("valid name");
        assert_ne!(first.id(), second.id());
        assert_eq!(first.id().raw(), 0);
        assert_eq!(second.id().raw(), 1);
    }

    #[test]
    fn wasm_callback_import_name_uses_shared_callback_lane() {
        assert_eq!(
            wasm_callback_import_name("method", "demo_listener", "on_event"),
            "__boltffi_callback_method_demo_listener_on_event"
        );
    }

    #[test]
    fn async_lifecycle_symbol_names_append_runtime_suffixes() {
        assert_eq!(
            async_lifecycle_symbol_name("boltffi_function_demo_spin", AsyncLifecycle::Poll),
            "boltffi_async_function_demo_spin_poll"
        );
        assert_eq!(
            async_lifecycle_symbol_name("boltffi_function_demo_spin", AsyncLifecycle::PollSync),
            "boltffi_async_function_demo_spin_poll_sync"
        );
        assert_eq!(
            async_lifecycle_symbol_name("boltffi_function_demo_spin", AsyncLifecycle::Complete),
            "boltffi_async_function_demo_spin_complete"
        );
    }

    #[test]
    fn wasm_async_callback_names_use_start_import_and_complete_export() {
        let slot = CallbackSlot::from_method_name("onEvent");
        assert_eq!(
            callback_wasm_import_start_name("demo::Listener", &slot),
            "__boltffi_callback_async_start_demo_listener_on_event"
        );
        assert_eq!(
            callback_wasm_complete_symbol_name("demo::Listener", &slot),
            "boltffi_callback_demo_listener_on_event_complete"
        );
    }
}

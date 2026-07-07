use boltffi::*;

/// A record with a field literally named `type`, requiring the Rust raw
/// identifier `r#type`.
///
/// The generated `WireEncode::encode_to` built its per-field scratch buffer
/// variable name by naively string-formatting the field identifier
/// (`format!("__boltffi_buf_{}", field_name)`), which for a raw identifier
/// includes the literal `r#` prefix and produces an invalid Rust
/// identifier (`__boltffi_buf_r#type`). `quote::format_ident!`, used
/// elsewhere in the same codegen for this exact purpose, unraws identifiers
/// automatically; this exists to guard against that inconsistency
/// regressing.
#[data]
#[derive(Clone, Debug, PartialEq)]
pub struct TypedEvent {
    pub id: i64,
    pub r#type: Option<String>,
}

#[export]
#[demo_bench_macros::demo_case(
    "records.keyword_fields.typed_event.should_roundtrip_raw_identifier_field",
    justification = "Ensure a record with a field literally named `type` (Rust raw identifier r#type) crosses the wire and returns unchanged.",
    directions = "Call `records::keyword_fields::echo_typed_event` through the generated binding and assert a record with a field literally named `type` crosses the wire and returns unchanged."
)]
pub fn echo_typed_event(event: TypedEvent) -> TypedEvent {
    event
}

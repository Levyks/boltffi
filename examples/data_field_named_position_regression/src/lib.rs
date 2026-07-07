//! Regression coverage: a `#[data]` struct or enum-variant field literally
//! named `position` broke the generated `WireDecode::decode_from` for the
//! non-blittable ("encoded") wire path.
//!
//! `decode_from` uses a fixed, unhygienic local variable named `position`
//! as its running byte-offset accumulator (`let mut position = 0usize; ...
//! position += size;`). A field named `position` gets destructured via
//! `let (position, size) = <FieldType>::decode_from(...)?;`, which shadows
//! the accumulator with the *decoded field value* instead — so the very
//! next `position += size;` tries to add a `usize` to the field's own type,
//! which fails to compile unless the field type happens to implement
//! `AddAssign<usize>` (and if it did, would silently produce a wrong
//! accumulated offset instead of a compile error).
//!
//! This crate is intentionally excluded from the main workspace (see
//! `Cargo.toml`'s root `exclude` list), matching `examples/demo` and
//! `examples/option_scalar_data_param_regression`.

use boltffi::*;

#[derive(Clone, Copy, Default)]
#[data]
#[repr(u8)]
pub enum Kind {
    #[default]
    A,
    B,
}

#[derive(Clone)]
#[data]
pub struct Record {
    pub message_id: u32,
    pub position: Kind,
    pub other: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use boltffi::__private::wire::{WireDecode, WireEncode};

    /// See https://github.com/boltffi/boltffi (issue TBD).
    #[test]
    fn field_named_position_round_trips_correctly() {
        let value = Record {
            message_id: 0xDEAD_BEEF,
            position: Kind::B,
            other: 0x1234,
        };
        let mut buf = vec![0u8; value.wire_size()];
        value.encode_to(&mut buf);
        let (decoded, consumed) = Record::decode_from(&buf).unwrap();
        assert_eq!(decoded.message_id, value.message_id);
        assert!(matches!(decoded.position, Kind::B));
        assert_eq!(decoded.other, value.other);
        assert_eq!(consumed, buf.len());
    }
}

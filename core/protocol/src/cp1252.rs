//! Windows-1252 <-> `str`, the encoding the client's `char` strings are in.
//!
//! The wire carries bytes, not Unicode: the packed string copies raw
//! bytes and the client renders them through the ANSI code page. ACE decodes with
//! `Encoding.Default`, which is the same thing on a western install
//! (`docs/formats/03-serialisation-primitives.md` §5.2).
//!
//! The table and its conversions are `dereth_primitives::text::cp1252`, the one copy in the
//! workspace; this module re-exports them at the path the protocol's callers have always used.

pub use dereth_primitives::text::cp1252::{
    byte_to_unit, decode, encode, encode_lossy, unit_to_byte, Cp1252, ACP,
};

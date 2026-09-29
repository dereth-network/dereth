//! `ACE.Common.Extensions`: one module per ACE file.
//!
//! `BinaryReaderExtensions` is ported next to the reader it extends, in
//! `crate::dotnet::binary_reader_extensions`.
//!
//! Not ported: `PropertyInfoExtensions` (reflection over custom attributes;
//! a `skipped` member rule of the port's ledger scope).

pub mod character_name_extensions;
pub mod date_time_extensions;
pub mod double_extensions;
pub mod enum_helper;
pub mod exception_extensions;
pub mod float_extensions;
pub mod list_extensions;
pub mod string_extensions;
pub mod time_span_extensions;

//! Localized text as the September 2013 Asheron's Call 2 client writes it: string references and
//! their variables, numbers in a language's format, and the string tables' meta language.
//!
//! **What it is.** The game says almost everything through string references: a token in a string
//! table, or a literal, with variables (numbers, or other references). This crate turns one into
//! text exactly as the client does: the variables written in the table's or the language's number
//! format ([`number`]), the table entry rendered with the string tables' meta language ([`meta`]:
//! plurals, choices by grammatical flags, capitals, item names), and the result unescaped; when it
//! cannot, the text is the client's own error message. Text is UTF-16, as the client keeps it.
//!
//! **How to use it.** Make a [`DatSource`] over the client's data files (the portal file first,
//! then the language's), build or read a [`StringInfo`], and ask for its text:
//!
//! ```no_run
//! # use std::sync::Arc;
//! # fn files() -> Vec<Arc<auberean_dat::DatFile>> { Vec::new() }
//! use auberean_text::{DatSource, StringInfo};
//! let mut source = DatSource::new(files());
//! let mut greeting = StringInfo::entry(0x2500_0001, 0x0000_1234);
//! let text: String = greeting.text(&mut source);
//! ```
//!
//! [`StringInfo::get_string`] answers the UTF-16 units. A reference read from a message or a script
//! converts field for field (the client runtime has the conversions).
//!
//! **Depends on** `auberean-assets` (the string table and string state records), `auberean-dat`
//! (the data files) and `auberean-primitives` (where a cell lies on the land, for a place's
//! text). **Used by** the script machine (`auberean-script`: a host offers its natives a
//! [`Source`]), the script natives (`auberean-glue`), physics (`auberean-physics`: numbers as
//! text), the client runtime and its SDK, and the client's panels.
//!
//! **Must never** reach a file, the network or a clock: it reads records only through a [`Source`],
//! so it builds for the browser.

pub mod crt;
pub mod info;
pub mod language;
pub mod meta;
pub mod names;
pub mod number;
pub mod place;
pub mod source;
pub mod wide;

pub use info::{Code, Entry, Source, StringInfo, Table, Value, Variable};
pub use language::{GoreOverrides, Language};
pub use source::DatSource;

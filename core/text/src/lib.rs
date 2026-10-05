//! Shared text tags, string-table resolution and metalanguage composition over an asset source.
//!
//! **Depends on** `dereth-primitives` for asset access and identifiers and `dereth-assets` for
//! string-table decoding and escaping. **Used by** `dereth-ui` and `dereth-classic-ui`.
//!
//! **Must never** locate files, reach the platform, lay out glyphs or dispatch widget events:
//! callers supply the asset source and decide how to display the resulting text.
//!
//! [`tag`] parses clickable text spans; [`metalanguage`] renders the table's conditional forms;
//! [`StringResolver`] separates raw table fragments from unescaped text; [`string_table`]
//! composes named or positional values and caches decoded tables from any asset source.

pub mod metalanguage;
pub mod resolver;
pub mod string_table;
pub mod tag;

pub use metalanguage::render;
pub use resolver::StringResolver;
pub use string_table::{render_named, render_positional, render_token, DatStringResolver};
pub use tag::{TagKind, TagSpan, TaggedText, TextTag};

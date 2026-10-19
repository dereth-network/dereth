//! Server content: ACE's SQL with symbols where ids go.
//!
//! A pack's SQL is ACE's own format, one file per object as ACE's `export-sql` writes it, with
//! `{{kind:symbol}}` wherever an id goes:
//!
//! | token | becomes |
//! |---|---|
//! | `{{wcid:guide}}` | the weenie class id, in decimal |
//! | `{{guid:guide@A9B4}}` | the id of an instance placed in landblock `A9B4`, in hex |
//! | `{{cell:hall@A9B4}}` | the id of the pack's indoor cell `hall` of landblock `A9B4`, in hex |
//! | `{{surface-texture:stripes}}` | a record's id, in hex (also `render-surface`, `surface`) |
//!
//! A symbol without a pack is the file's own pack's. A token that does not read stops the build
//! with its file and line. Everything else in the file is left exactly as it is.

use std::ops::Range;
use std::path::Path;

use crate::error::{Error, Loc};
use crate::symbol::{self, Kind};

/// One `{{kind:symbol}}` in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    /// The symbol, in full; an instance's with its landblock in upper case.
    pub symbol: String,
    /// For an instance, its landblock.
    pub landblock: Option<u16>,
    pub at: Loc,
    range: Range<usize>,
}

/// Every token in `text`, the file `path` of the pack `pack`.
///
/// # Errors
/// A token that does not read: an unknown kind, a symbol that is not one, an instance without its
/// landblock, a `{{` with no `}}`.
pub fn tokens(path: &Path, pack: &str, text: &str) -> Result<Vec<Token>, Error> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("{{") {
        let start = from + i;
        let at = Loc::at(path, text, start);
        let Some(j) = text[start + 2..].find("}}") else {
            return Err(Error::source(&at, "a {{ with no }}"));
        };
        let end = start + 2 + j + 2;
        let inner = text[start + 2..end - 2].trim();
        let (kind, sym) = inner
            .split_once(':')
            .ok_or_else(|| Error::source(&at, format!("{{{{{inner}}}}} is not kind:symbol")))?;
        let kind = Kind::parse(kind.trim()).ok_or_else(|| {
            Error::source(
                &at,
                format!(
                    "{kind:?} is not a kind: wcid, guid, cell, render-surface, surface-texture or surface"
                ),
            )
        })?;
        let mut symbol =
            symbol::qualify(pack, sym.trim()).map_err(|why| Error::source(&at, why))?;
        let landblock =
            symbol::instance_landblock(&symbol).map_err(|why| Error::source(&at, why))?;
        match (kind, landblock) {
            (Kind::Guid, None) => {
                return Err(Error::source(
                    &at,
                    "an instance names its landblock, as {{guid:guide@A9B4}}",
                ))
            }
            (Kind::Cell, None) => {
                return Err(Error::source(
                    &at,
                    "a cell names its landblock, as {{cell:hall@A9B4}}",
                ))
            }
            (Kind::Guid | Kind::Cell, Some(lb)) => {
                if (lb >> 8) == 0xFF || (lb & 0xFF) == 0xFF {
                    return Err(Error::source(&at, format!("{lb:04X} is not a landblock")));
                }
                symbol = symbol::canonical_instance(&symbol, lb);
            }
            (_, Some(_)) => {
                return Err(Error::source(
                    &at,
                    "only an instance or a cell names a landblock",
                ))
            }
            (_, None) => {}
        }
        out.push(Token {
            kind,
            symbol,
            landblock,
            at,
            range: start..end,
        });
        from = end;
    }
    Ok(out)
}

/// `text` with every token replaced by the id `id` gives it.
#[must_use]
pub fn fill(text: &str, tokens: &[Token], id: impl Fn(&Token) -> u32) -> String {
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for t in tokens {
        out.push_str(&text[from..t.range.start]);
        let v = id(t);
        if t.kind.hex() {
            out.push_str(&format!("0x{v:08X}"));
        } else {
            out.push_str(&v.to_string());
        }
        from = t.range.end;
    }
    out.push_str(&text[from..]);
    out
}

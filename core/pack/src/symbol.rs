//! Symbols: how a pack names what it makes, so that ids are given out per world.
//!
//! A symbol is `pack:name`. Inside its own pack a bare `name` means the pack's own; another pack's
//! is named in full. A pack name and a symbol name are lowercase letters, digits and hyphens,
//! starting with a letter or a digit. An instance placed in the world is bound to its landblock,
//! which its symbol carries: `guide@A9B4`.
//!
//! One name may stand for several kinds at once: a class `guide` and the instance `guide@A9B4`
//! are two symbols. What kinds there are is the game's ([`SymbolKind`]).

use std::fmt;
use std::hash::Hash;

/// How new ids of a kind are given out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Numbering {
    /// A record of the data files: a new id from this span, inclusive.
    Record { first: u32, last: u32 },
    /// Numbered inside the landblock the symbol names (`name@XXYY`).
    InLandblock,
    /// Counted up from `start`, the lowest the lockfile does not hold. When `world_sets`, a
    /// world's `[ids]` table can start it elsewhere.
    Counted { start: u32, world_sets: bool },
}

/// What a game's symbols can name: a table the game supplies, each kind with its name and how
/// it is numbered.
pub trait SymbolKind:
    Copy + Ord + Eq + Hash + fmt::Debug + fmt::Display + Send + Sync + 'static
{
    /// Every kind, in the order the lockfile lists them.
    const KINDS: &'static [Self];

    /// How sources and the lockfile write the kind.
    fn kind_name(self) -> &'static str;

    /// How new ids of the kind are given out.
    fn numbering(self) -> Numbering;

    /// Whether ids of this kind are written in hexadecimal.
    fn written_in_hex(self) -> bool;

    /// The kind a source names.
    #[must_use]
    fn from_name(s: &str) -> Option<Self> {
        Self::KINDS.iter().copied().find(|k| k.kind_name() == s)
    }

    /// For the kinds that are records: the id span their new ids are taken from, inclusive.
    #[must_use]
    fn record_span(self) -> Option<(u32, u32)> {
        match self.numbering() {
            Numbering::Record { first, last } => Some((first, last)),
            _ => None,
        }
    }

    /// Whether this kind is a record in the data files.
    #[must_use]
    fn names_record(self) -> bool {
        self.record_span().is_some()
    }
}

/// Whether `s` is a pack name or a symbol's name: lowercase letters, digits and hyphens, starting
/// with a letter or a digit.
#[must_use]
pub fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// `name` as written in `pack`, as the full `pack:name`.
///
/// # Errors
/// A name that is not one, with the reason.
pub fn qualify(pack: &str, name: &str) -> Result<String, String> {
    let (p, n) = match name.split_once(':') {
        Some((p, n)) => (p, n),
        None => (pack, name),
    };
    if !is_name(p) {
        return Err(format!("{p:?} is not a pack name"));
    }
    let bare = n.split_once('@').map_or(n, |(b, _)| b);
    if !is_name(bare) {
        return Err(format!(
            "{n:?} is not a symbol: lowercase letters, digits and hyphens"
        ));
    }
    Ok(format!("{p}:{n}"))
}

/// An instance symbol's landblock, from its `@XXYY`: `None` when it names none.
///
/// # Errors
/// An `@` not followed by four hexadecimal digits.
pub fn instance_landblock(symbol: &str) -> Result<Option<u16>, String> {
    let Some((_, lb)) = symbol.rsplit_once('@') else {
        return Ok(None);
    };
    if lb.len() != 4 {
        return Err(format!(
            "{symbol:?}: a landblock is four hex digits, as A9B4"
        ));
    }
    u16::from_str_radix(lb, 16)
        .map(Some)
        .map_err(|_| format!("{symbol:?}: a landblock is four hex digits, as A9B4"))
}

/// An instance symbol with its landblock written the one way the lockfile keeps it, upper case.
#[must_use]
pub fn canonical_instance(symbol: &str, landblock: u16) -> String {
    let bare = symbol.rsplit_once('@').map_or(symbol, |(b, _)| b);
    format!("{bare}@{landblock:04X}")
}

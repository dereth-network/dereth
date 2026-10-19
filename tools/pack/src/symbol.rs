//! Symbols: how a pack names what it makes, so that ids are given out per world.
//!
//! A symbol is `pack:name`. Inside its own pack a bare `name` means the pack's own; another pack's
//! is named in full. A pack name and a symbol name are lowercase letters, digits and hyphens,
//! starting with a letter or a digit. An instance placed in the world is bound to its landblock,
//! which its symbol carries: `guide@A9B4`.
//!
//! One name may stand for several kinds at once: the weenie `guide` and the instance
//! `guide@A9B4` are two symbols.

use std::fmt;

/// What a symbol names, and so where its number comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A picture record (`0x06`).
    RenderSurface,
    /// An image texture (`0x05`): the images of one texture, largest first.
    SurfaceTexture,
    /// A surface (`0x08`): how a texture or a colour is laid on a polygon.
    Surface,
    /// A weenie class id, numbered in the server's content.
    Wcid,
    /// A placed instance's id, `0x7XXYYnnn` in its landblock.
    Guid,
}

impl Kind {
    /// Every kind, in the order the lockfile lists them.
    pub const ALL: [Self; 5] = [
        Self::RenderSurface,
        Self::SurfaceTexture,
        Self::Surface,
        Self::Wcid,
        Self::Guid,
    ];

    /// How sources and the lockfile write the kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::RenderSurface => "render-surface",
            Self::SurfaceTexture => "surface-texture",
            Self::Surface => "surface",
            Self::Wcid => "wcid",
            Self::Guid => "guid",
        }
    }

    /// The kind a source names.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == s)
    }

    /// For the kinds that are records: the id span their new ids are taken from, inclusive.
    /// Surfaces stay in their 16-bit span, since cells name them by their low 16 bits; pictures
    /// stay below the client's own records at `0x06F00000`.
    #[must_use]
    pub const fn span(self) -> Option<(u32, u32)> {
        match self {
            Self::RenderSurface => Some((0x0600_0000, 0x06EF_FFFF)),
            Self::SurfaceTexture => Some((0x0500_0000, 0x05FF_FFFF)),
            Self::Surface => Some((0x0800_0000, 0x0800_FFFF)),
            Self::Wcid | Self::Guid => None,
        }
    }

    /// Whether ids of this kind are written in hexadecimal: every kind but the weenie class,
    /// which ACE's own SQL writes in decimal.
    #[must_use]
    pub const fn hex(self) -> bool {
        !matches!(self, Self::Wcid)
    }

    /// Whether this kind is a record in the data files.
    #[must_use]
    pub const fn is_record(self) -> bool {
        self.span().is_some()
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
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

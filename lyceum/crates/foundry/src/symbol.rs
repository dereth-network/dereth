//! Symbols: how a pack names what it makes, so that ids are given out per world.
//!
//! A symbol is `pack:name`. Inside its own pack a bare `name` means the pack's own; another pack's
//! is named in full. A pack name and a symbol name are lowercase letters, digits and hyphens,
//! starting with a letter or a digit. An instance placed in the world is bound to its landblock,
//! which its symbol carries: `guide@A9B4`.
//!
//! One name may stand for several kinds at once: the weenie `guide` and the instance
//! `guide@A9B4` are two symbols.
//!
//! The grammar of symbols is the shared pack format's (`dereth_pack::symbol`); the kinds here are
//! Lyceum's, its game's table of what a symbol can name.

use std::fmt;

pub use dereth_pack::symbol::{canonical_instance, instance_landblock, is_name, qualify};
use dereth_pack::Numbering;

/// What a symbol names, and so where its number comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A mesh record (`0x01`).
    GfxObj,
    /// A composite model (`0x02`).
    Setup,
    /// An indoor piece record (`0x0D`).
    Environment,
    /// A model's distance bands (`0x11`).
    Degrade,
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
    /// A new indoor cell's id, `0xXXYYnnnn` in its landblock.
    Cell,
    /// A loot table's id, numbered in the server's content: a death treasure's or a wielded
    /// treasure's, which share their ids.
    Treasure,
}

impl Kind {
    /// Every kind, in the order the lockfile lists them.
    pub const ALL: [Self; 11] = [
        Self::GfxObj,
        Self::Setup,
        Self::Environment,
        Self::Degrade,
        Self::RenderSurface,
        Self::SurfaceTexture,
        Self::Surface,
        Self::Wcid,
        Self::Guid,
        Self::Cell,
        Self::Treasure,
    ];

    /// How sources and the lockfile write the kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::GfxObj => "gfxobj",
            Self::Setup => "setup",
            Self::Environment => "environment",
            Self::Degrade => "degrade",
            Self::RenderSurface => "render-surface",
            Self::SurfaceTexture => "surface-texture",
            Self::Surface => "surface",
            Self::Wcid => "wcid",
            Self::Guid => "guid",
            Self::Cell => "cell",
            Self::Treasure => "treasure",
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
            Self::GfxObj => Some((0x0100_0000, 0x01FF_FFFF)),
            Self::Setup => Some((0x0200_0000, 0x02FF_FFFF)),
            Self::Environment => Some((0x0D00_0000, 0x0D00_FFFF)),
            Self::Degrade => Some((0x1100_0000, 0x11FF_FFFF)),
            Self::RenderSurface => Some((0x0600_0000, 0x06EF_FFFF)),
            Self::SurfaceTexture => Some((0x0500_0000, 0x05FF_FFFF)),
            Self::Surface => Some((0x0800_0000, 0x0800_FFFF)),
            Self::Wcid | Self::Guid | Self::Cell | Self::Treasure => None,
        }
    }

    /// Whether ids of this kind are written in hexadecimal: every kind but the weenie class and
    /// the loot table, which ACE's own SQL writes in decimal.
    #[must_use]
    pub const fn hex(self) -> bool {
        !matches!(self, Self::Wcid | Self::Treasure)
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

impl dereth_pack::SymbolKind for Kind {
    const KINDS: &'static [Self] = &Self::ALL;

    fn kind_name(self) -> &'static str {
        self.name()
    }

    fn numbering(self) -> Numbering {
        match self {
            Self::Wcid => Numbering::Counted {
                start: crate::world::DEFAULT_WCID_START,
                world_sets: true,
            },
            Self::Treasure => Numbering::Counted {
                start: crate::loot::NEW_ID,
                world_sets: false,
            },
            Self::Guid | Self::Cell => Numbering::InLandblock,
            k => k
                .span()
                .map_or(Numbering::InLandblock, |(first, last)| Numbering::Record {
                    first,
                    last,
                }),
        }
    }

    fn written_in_hex(self) -> bool {
        self.hex()
    }
}

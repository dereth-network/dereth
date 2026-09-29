//! Identifiers. Newtypes rather than bare integers, because the client has five different 32-bit id
//! spaces and mixing them is the easiest bug to write and the hardest to see.

use std::fmt;

macro_rules! id_newtype {
    ($(#[$m:meta])* $name:ident, $inner:ty) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name(pub $inner);

        impl $name {
            #[inline]
            #[must_use]
            pub const fn raw(self) -> $inner { self.0 }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!(stringify!($name), "(0x{:08X})"), self.0)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "0x{:08X}", self.0)
            }
        }
    };
}

id_newtype!(
    /// An object inside one of the four dat files. The top byte selects the type; see
    /// `docs/formats/02-file-ids-and-types.md`.
    DataId, u32
);

id_newtype!(
    /// A world object. ACE calls this a GUID.
    ObjectId, u32
);

id_newtype!(
    /// A cell: `landblock << 16 | cell index`. Index `0x0001..=0x0040` is one of the 64 outdoor land
    /// cells, `0x0100..=0xFFFD` an interior cell, `0xFFFE` the landblock info, `0xFFFF` the landblock.
    CellId, u32
);

id_newtype!(
    /// A property key. The client works with bare numeric ids: it ships no id-to-name table, so the
    /// names everyone uses come from the emulator projects.
    PropertyId, u32
);

/// The high half of a cell id: `blockX << 8 | blockY`, both `0..=0xFE`.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct LandblockId(pub u16);

impl LandblockId {
    #[inline]
    #[must_use]
    pub const fn new(x: u8, y: u8) -> Self {
        Self(((x as u16) << 8) | y as u16)
    }

    #[inline]
    #[must_use]
    pub const fn x(self) -> u8 {
        (self.0 >> 8) as u8
    }

    #[inline]
    #[must_use]
    pub const fn y(self) -> u8 {
        (self.0 & 0xFF) as u8
    }

    /// The `0xXXYYFFFF` record holding this block's terrain.
    #[inline]
    #[must_use]
    pub const fn terrain_id(self) -> DataId {
        DataId(((self.0 as u32) << 16) | 0xFFFF)
    }

    /// The `0xXXYYFFFE` record holding this block's static objects, buildings and portal graph.
    #[inline]
    #[must_use]
    pub const fn info_id(self) -> DataId {
        DataId(((self.0 as u32) << 16) | 0xFFFE)
    }

    /// The cell id of one of this block's interior cells.
    #[inline]
    #[must_use]
    pub const fn cell(self, index: u16) -> CellId {
        CellId(((self.0 as u32) << 16) | index as u32)
    }
}

impl fmt::Debug for LandblockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LandblockId(0x{:04X})", self.0)
    }
}

impl fmt::Display for LandblockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04X}", self.0)
    }
}

impl CellId {
    /// The landblock this cell belongs to.
    #[inline]
    #[must_use]
    pub const fn landblock(self) -> LandblockId {
        LandblockId((self.0 >> 16) as u16)
    }

    /// The index within the landblock.
    #[inline]
    #[must_use]
    pub const fn index(self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// True for the 64 generated outdoor cells, which are never stored in the dat.
    #[inline]
    #[must_use]
    pub const fn is_outdoor(self) -> bool {
        let i = self.index();
        i >= 0x0001 && i <= 0x0040
    }
}

/// The dat object types, keyed by the top byte of a [`DataId`].
///
/// This is deliberately not exhaustive: the full 65-entry registration table is in
/// `docs/formats/02-file-ids-and-types.md`, and the dat crate owns the complete
/// mapping. These are the variants the seams need to name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DataType {
    GfxObj,
    Setup,
    Animation,
    Palette,
    SurfaceTexture,
    Texture,
    Surface,
    MotionTable,
    Wave,
    Environment,
    PaletteSet,
    ClothingTable,
    DegradeInfo,
    Scene,
    Region,
    SoundTable,
    ParticleEmitter,
    PhysicsScript,
    PhysicsScriptTable,
    Landblock,
    LandblockInfo,
    EnvCell,
    /// Anything the seams have not yet needed to name.
    Other(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landblock_ids_round_trip() {
        let lb = LandblockId::new(0xA9, 0xB4);
        assert_eq!(lb.0, 0xA9B4);
        assert_eq!(lb.x(), 0xA9);
        assert_eq!(lb.y(), 0xB4);
        assert_eq!(lb.terrain_id(), DataId(0xA9B4_FFFF));
        assert_eq!(lb.info_id(), DataId(0xA9B4_FFFE));
        assert_eq!(lb.cell(0x0100), CellId(0xA9B4_0100));
    }

    #[test]
    fn cell_ids_split_correctly() {
        let c = CellId(0xA9B4_0100);
        assert_eq!(c.landblock(), LandblockId(0xA9B4));
        assert_eq!(c.index(), 0x0100);
        assert!(!c.is_outdoor());
        assert!(CellId(0xA9B4_0001).is_outdoor());
        assert!(CellId(0xA9B4_0040).is_outdoor());
        assert!(!CellId(0xA9B4_0041).is_outdoor());
    }

    #[test]
    fn ids_of_different_spaces_do_not_mix() {
        // This is a compile-time property; the test documents the intent.
        let d = DataId(1);
        let o = ObjectId(1);
        assert_eq!(d.raw(), o.raw());
        // `assert_eq!(d, o)` would not compile, which is the point of the newtypes.
    }
}

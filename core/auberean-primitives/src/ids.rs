//! The ids of the data and the world.

use core::fmt;

/// The id of a record in the data files: a 32-bit number whose range says what kind of record it
/// is (the type is derived from the id, never stored with it).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct DataId(pub u32);

impl DataId {
    /// The id that names nothing.
    pub const NONE: Self = Self(0);

    /// Whether this id names nothing.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Debug for DataId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DataId({:08X})", self.0)
    }
}

impl fmt::Display for DataId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}", self.0)
    }
}

impl From<u32> for DataId {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

/// A cell of the world, `0xXXYYnnnn`: the landblock's column `XX` and row `YY` on a 256 x 256
/// grid, and a low word saying which part of the landblock. The low words `0xFFFF` (the landblock's
/// heights), `0xFFFE` (its placed objects), `0xFFFD` (its path map) and `0xFFFC` (its light) name
/// the landblock's records; `0x0100` and up are its interior cells.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CellId(pub u32);

impl CellId {
    /// The first low word of an interior cell.
    pub const FIRST_INTERIOR: u16 = 0x0100;

    /// The landblock's column.
    #[must_use]
    pub const fn block_x(self) -> u8 {
        self.0.to_be_bytes()[0]
    }

    /// The landblock's row.
    #[must_use]
    pub const fn block_y(self) -> u8 {
        self.0.to_be_bytes()[1]
    }

    /// The low word: which record or cell of the landblock.
    #[must_use]
    pub const fn low(self) -> u16 {
        let b = self.0.to_be_bytes();
        u16::from_be_bytes([b[2], b[3]])
    }

    /// The same landblock with another low word.
    #[must_use]
    pub const fn with_low(self, low: u16) -> Self {
        Self((self.0 & 0xFFFF_0000) | low as u32)
    }

    /// Whether this names an interior cell rather than one of the landblock's own records.
    #[must_use]
    pub const fn is_interior(self) -> bool {
        let low = self.low();
        low >= Self::FIRST_INTERIOR && low < 0xFFFC
    }
}

impl fmt::Debug for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CellId({:08X})", self.0)
    }
}

impl fmt::Display for CellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}", self.0)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (plain data conversions)
    use super::*;

    #[test]
    fn a_cell_id_splits_into_column_row_and_low_word() {
        let c = CellId(0x5D3A_0104);
        assert_eq!((c.block_x(), c.block_y(), c.low()), (0x5D, 0x3A, 0x0104));
        assert!(c.is_interior());
        assert_eq!(c.with_low(0xFFFF), CellId(0x5D3A_FFFF));
        assert!(!c.with_low(0xFFFE).is_interior());
    }
}

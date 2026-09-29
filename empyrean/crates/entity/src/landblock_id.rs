// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/LandblockId.cs
//! `LandblockId`: a cell id (`0xXXYYcccc`) viewed as a landblock.

// C# narrowing casts (`(byte)`, `(ushort)`) are two's-complement truncation, i.e. `as`.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::fmt;
use std::hash::{Hash, Hasher};

/// `Convert.ToByte(int)`: checked, throwing `OverflowException` outside `0..=255`.
fn convert_to_byte(value: i32) -> u8 {
    u8::try_from(value).unwrap_or_else(|_| {
        panic!("OverflowException: Value was either too large or too small for an unsigned byte.")
    })
}

/// ACE: LandblockId. A value type; `==` compares only [`LandblockId::landblock`] (the upper 16
/// bits), as ACE's `operator ==` and `Equals` do.
// ACE: LandblockId.Raw
#[derive(Clone, Copy, Default)]
pub struct LandblockId {
    raw: u32,
}

impl LandblockId {
    // ACE: LandblockId.LandblockId
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        LandblockId { raw }
    }

    // ACE: LandblockId.LandblockId
    #[must_use]
    pub const fn from_xy(x: u8, y: u8) -> Self {
        LandblockId {
            raw: (x as u32) << 24 | (y as u32) << 16,
        }
    }

    // ACE: LandblockId.Raw
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.raw
    }

    /// # Panics
    /// With .NET's `OverflowException` when `LandblockX` is 255, as `Convert.ToByte` throws.
    // ACE: LandblockId.East
    #[must_use]
    pub fn east(self) -> LandblockId {
        LandblockId::from_xy(
            convert_to_byte(i32::from(self.landblock_x()) + 1),
            self.landblock_y(),
        )
    }

    /// # Panics
    /// When `LandblockX` is 0 (`Convert.ToByte` overflow).
    // ACE: LandblockId.West
    #[must_use]
    pub fn west(self) -> LandblockId {
        LandblockId::from_xy(
            convert_to_byte(i32::from(self.landblock_x()) - 1),
            self.landblock_y(),
        )
    }

    /// # Panics
    /// When `LandblockY` is 255 (`Convert.ToByte` overflow).
    // ACE: LandblockId.North
    #[must_use]
    pub fn north(self) -> LandblockId {
        LandblockId::from_xy(
            self.landblock_x(),
            convert_to_byte(i32::from(self.landblock_y()) + 1),
        )
    }

    /// # Panics
    /// When `LandblockY` is 0 (`Convert.ToByte` overflow).
    // ACE: LandblockId.South
    #[must_use]
    pub fn south(self) -> LandblockId {
        LandblockId::from_xy(
            self.landblock_x(),
            convert_to_byte(i32::from(self.landblock_y()) - 1),
        )
    }

    /// # Panics
    /// On a `Convert.ToByte` overflow at the map edge.
    // ACE: LandblockId.NorthEast
    #[must_use]
    pub fn north_east(self) -> LandblockId {
        LandblockId::from_xy(
            convert_to_byte(i32::from(self.landblock_x()) + 1),
            convert_to_byte(i32::from(self.landblock_y()) + 1),
        )
    }

    /// # Panics
    /// On a `Convert.ToByte` overflow at the map edge.
    // ACE: LandblockId.NorthWest
    #[must_use]
    pub fn north_west(self) -> LandblockId {
        LandblockId::from_xy(
            convert_to_byte(i32::from(self.landblock_x()) - 1),
            convert_to_byte(i32::from(self.landblock_y()) + 1),
        )
    }

    /// # Panics
    /// On a `Convert.ToByte` overflow at the map edge.
    // ACE: LandblockId.SouthEast
    #[must_use]
    pub fn south_east(self) -> LandblockId {
        LandblockId::from_xy(
            convert_to_byte(i32::from(self.landblock_x()) + 1),
            convert_to_byte(i32::from(self.landblock_y()) - 1),
        )
    }

    /// # Panics
    /// On a `Convert.ToByte` overflow at the map edge.
    // ACE: LandblockId.SouthWest
    #[must_use]
    pub fn south_west(self) -> LandblockId {
        LandblockId::from_xy(
            convert_to_byte(i32::from(self.landblock_x()) - 1),
            convert_to_byte(i32::from(self.landblock_y()) - 1),
        )
    }

    // ACE: LandblockId.Landblock
    #[must_use]
    pub const fn landblock(self) -> u16 {
        ((self.raw >> 16) & 0xFFFF) as u16
    }

    // ACE: LandblockId.LandblockX
    #[must_use]
    pub const fn landblock_x(self) -> u8 {
        ((self.raw >> 24) & 0xFF) as u8
    }

    // ACE: LandblockId.LandblockY
    #[must_use]
    pub const fn landblock_y(self) -> u8 {
        ((self.raw >> 16) & 0xFF) as u8
    }

    /// Only used to calculate `LandcellX` and `LandcellY`. Cell 0 wraps to 255 through the
    /// `(byte)` cast.
    // ACE: LandblockId.Landcell
    #[must_use]
    pub const fn landcell(self) -> u16 {
        (self.raw & 0x3F).wrapping_sub(1) as u8 as u16
    }

    // ACE: LandblockId.LandcellX
    #[must_use]
    pub const fn landcell_x(self) -> u8 {
        ((self.landcell() >> 3) & 0x7) as u8
    }

    // ACE: LandblockId.LandcellY
    #[must_use]
    pub const fn landcell_y(self) -> u8 {
        (self.landcell() & 0x7) as u8
    }

    // ACE: LandblockId.Indoors
    #[must_use]
    pub const fn indoors(self) -> bool {
        (self.raw & 0xFFFF) >= 0x100
    }

    // ACE: LandblockId.IsAdjacentTo
    #[must_use]
    pub fn is_adjacent_to(self, block: LandblockId) -> bool {
        (i32::from(self.landblock_x()) - i32::from(block.landblock_x())).abs() <= 1
            && (i32::from(self.landblock_y()) - i32::from(block.landblock_y())).abs() <= 1
    }

    // ACE: LandblockId.TransitionX
    #[must_use]
    pub fn transition_x(self, block_offset: i32) -> Option<LandblockId> {
        let new_x = i32::from(self.landblock_x()).wrapping_add(block_offset);
        if !(0..=254).contains(&new_x) {
            None
        } else {
            Some(LandblockId::new(
                (new_x as u32) << 24 | u32::from(self.landblock_y()) << 16 | self.raw & 0xFFFF,
            ))
        }
    }

    // ACE: LandblockId.TransitionY
    #[must_use]
    pub fn transition_y(self, block_offset: i32) -> Option<LandblockId> {
        let new_y = i32::from(self.landblock_y()).wrapping_add(block_offset);
        if !(0..=254).contains(&new_y) {
            None
        } else {
            Some(LandblockId::new(
                u32::from(self.landblock_x()) << 24 | (new_y as u32) << 16 | self.raw & 0xFFFF,
            ))
        }
    }
}

// ACE: LandblockId.op_Equality, LandblockId.op_Inequality, LandblockId.Equals
impl PartialEq for LandblockId {
    fn eq(&self, other: &Self) -> bool {
        self.landblock() == other.landblock()
    }
}

impl Eq for LandblockId {}

/// DIVERGE: ACE's `GetHashCode` is `base.GetHashCode()` (all of `Raw`), which disagrees with its
/// `==`; Rust's `Hash` must agree with `Eq`, so this hashes the landblock. ACE never hashes a
/// `LandblockId`.
// ACE: LandblockId.GetHashCode
impl Hash for LandblockId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.landblock().hash(state);
    }
}

// ACE: LandblockId.ToString
impl fmt::Display for LandblockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:08X}", self.raw)
    }
}

impl fmt::Debug for LandblockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LandblockId({:08X})", self.raw)
    }
}

//! Compass identities shared by block geometry and ambient placement.

/// Numeric compass direction, including the unassigned sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u32)]
pub enum BlockDirection {
    InViewerBlock = 0,
    North = 1,
    South = 2,
    East = 3,
    West = 4,
    NorthWest = 5,
    SouthWest = 6,
    NorthEast = 7,
    SouthEast = 8,
    Unknown = 9,
}

impl BlockDirection {
    /// True for N, NW and NE — the three directions whose "outward" edge is the block's max-y edge.
    #[must_use]
    pub const fn has_north(self) -> bool {
        matches!(self, Self::North | Self::NorthWest | Self::NorthEast)
    }
    /// True for S, SW and SE.
    #[must_use]
    pub const fn has_south(self) -> bool {
        matches!(self, Self::South | Self::SouthWest | Self::SouthEast)
    }
    /// True for E, NE and SE.
    #[must_use]
    pub const fn has_east(self) -> bool {
        matches!(self, Self::East | Self::NorthEast | Self::SouthEast)
    }
    /// True for W, NW and SW.
    #[must_use]
    pub const fn has_west(self) -> bool {
        matches!(self, Self::West | Self::NorthWest | Self::SouthWest)
    }
    /// True for the four cardinals, which is the guard on the inward crack fix of stitching.
    #[must_use]
    pub const fn is_cardinal(self) -> bool {
        matches!(self, Self::North | Self::South | Self::East | Self::West)
    }
}

impl BlockDirection {
    /// Select a compass direction from the signs of a grid delta.
    #[must_use]
    pub fn from_delta(dx: i32, dy: i32) -> Self {
        if dx < 0 {
            if dy < 0 {
                return Self::SouthWest;
            }
            return if dy > 0 { Self::NorthWest } else { Self::West };
        }
        if dx < 1 {
            if dy < 0 {
                return Self::South;
            }
            return if dy > 0 {
                Self::North
            } else {
                Self::InViewerBlock
            };
        }
        if dy < 0 {
            return Self::SouthEast;
        }
        if dy > 0 {
            Self::NorthEast
        } else {
            Self::East
        }
    }

    /// The rounded heading table, in radians clockwise from north.
    #[must_use]
    #[allow(clippy::approx_constant)] // Stored rounded heading values.
    pub fn heading_rad(self) -> f32 {
        match self {
            Self::North => 0.0,
            Self::NorthEast => 0.785_398_2,
            Self::East => 1.570_796_4,
            Self::SouthEast => 2.356_194_5,
            Self::South => 3.141_592_7,
            Self::SouthWest => 3.926_990_7,
            Self::West => 4.712_389,
            Self::NorthWest => 5.497_787,
            Self::InViewerBlock | Self::Unknown => 0.0,
        }
    }
}

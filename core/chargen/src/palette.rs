//! Representative colors for the supported character palette layouts.

/// The layout carried by a decoded palette, independently of the active interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteLayout {
    Indexed256,
    Extended2048,
}

impl PaletteLayout {
    #[must_use]
    pub const fn from_entry_count(count: usize) -> Option<Self> {
        match count {
            256 => Some(Self::Indexed256),
            2048 => Some(Self::Extended2048),
            _ => None,
        }
    }
}

/// The body or clothing part represented by a color selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaletteSample {
    Skin,
    Hair,
    Eyes,
    Headgear,
    Shirt,
    Trousers,
    Footwear,
}

impl PaletteSample {
    #[must_use]
    pub const fn index(self, layout: PaletteLayout) -> usize {
        match layout {
            PaletteLayout::Indexed256 => match self {
                Self::Skin => 12,
                Self::Hair => 30,
                Self::Eyes => 35,
                Self::Headgear => 253,
                Self::Shirt => 49,
                Self::Trousers => 68,
                Self::Footwear => 164,
            },
            PaletteLayout::Extended2048 => match self {
                Self::Skin => 0xB0,
                Self::Hair => 0xD0,
                Self::Eyes => 0x103,
                Self::Headgear | Self::Shirt | Self::Trousers | Self::Footwear => 0x520,
            },
        }
    }
}

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RegenLocationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RegenLocationType.cs`; do not edit by hand

/// ACE enum `RegenLocationType` (`[Flags]`), underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RegenLocationType(pub u32);

#[allow(non_upper_case_globals)]
impl RegenLocationType {
    pub const Undef: Self = Self(0x0);
    pub const OnTop: Self = Self(0x1);
    pub const Scatter: Self = Self(0x2);
    pub const Specific: Self = Self(0x4);
    pub const Contain: Self = Self(0x8);
    pub const Wield: Self = Self(0x10);
    pub const Shop: Self = Self(0x20);
    pub const Treasure: Self = Self(0x40);
    pub const Checkpoint: Self = Self(0x38);
    pub const OnTopTreasure: Self = Self(0x41);
    pub const ScatterTreasure: Self = Self(0x42);
    pub const SpecificTreasure: Self = Self(0x44);
    pub const ContainTreasure: Self = Self(0x48);
    pub const WieldTreasure: Self = Self(0x50);
    pub const ShopTreasure: Self = Self(0x60);
}

impl RegenLocationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::OnTop, Self::Scatter, Self::Specific, Self::Contain, Self::Wield, Self::Shop, Self::Checkpoint, Self::Treasure, Self::OnTopTreasure, Self::ScatterTreasure, Self::SpecificTreasure, Self::ContainTreasure, Self::WieldTreasure, Self::ShopTreasure];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "OnTop", "Scatter", "Specific", "Contain", "Wield", "Shop", "Checkpoint", "Treasure", "OnTopTreasure", "ScatterTreasure", "SpecificTreasure", "ContainTreasure", "WieldTreasure", "ShopTreasure"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[7, 4, 12, 1, 9, 2, 10, 6, 14, 3, 11, 8, 0, 5, 13];
}

super::support::ace_enum!(RegenLocationType, u32, flags);
super::support::ace_enum_from!(RegenLocationType, u32 => u64, i64);

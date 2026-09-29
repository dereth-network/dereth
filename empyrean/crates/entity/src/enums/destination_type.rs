// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/DestinationType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/DestinationType.cs`; do not edit by hand

/// ACE enum `DestinationType` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DestinationType(pub i32);

#[allow(non_upper_case_globals)]
impl DestinationType {
    pub const Undef: Self = Self(0x0);
    pub const Contain: Self = Self(0x1);
    pub const Wield: Self = Self(0x2);
    pub const Shop: Self = Self(0x4);
    pub const Treasure: Self = Self(0x8);
    pub const HouseBuy: Self = Self(0x10);
    pub const HouseRent: Self = Self(0x20);
    pub const Checkpoint: Self = Self(0x7);
    pub const ContainTreasure: Self = Self(0x9);
    pub const WieldTreasure: Self = Self(0xA);
    pub const ShopTreasure: Self = Self(0xC);
}

impl DestinationType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Contain, Self::Wield, Self::Shop, Self::Checkpoint, Self::Treasure, Self::ContainTreasure, Self::WieldTreasure, Self::ShopTreasure, Self::HouseBuy, Self::HouseRent];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Contain", "Wield", "Shop", "Checkpoint", "Treasure", "ContainTreasure", "WieldTreasure", "ShopTreasure", "HouseBuy", "HouseRent"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 6, 9, 10, 3, 8, 5, 0, 2, 7];
}

super::support::ace_enum!(DestinationType, i32, flags);
super::support::ace_enum_from!(DestinationType, i32 => i64);

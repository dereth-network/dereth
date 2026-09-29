// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/TreasureItemCategory.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/TreasureItemCategory.cs`; do not edit by hand

/// ACE enum `TreasureItemCategory`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreasureItemCategory(pub i32);

#[allow(non_upper_case_globals)]
impl TreasureItemCategory {
    pub const Undef: Self = Self(0);
    pub const Item: Self = Self(1);
    pub const MagicItem: Self = Self(2);
    pub const MundaneItem: Self = Self(3);
}

impl TreasureItemCategory {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Item, Self::MagicItem, Self::MundaneItem];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Item", "MagicItem", "MundaneItem"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 0];
}

super::support::ace_enum!(TreasureItemCategory, i32, plain);
super::support::ace_enum_from!(TreasureItemCategory, i32 => i64);

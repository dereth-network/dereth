// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/TreasureType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/TreasureType.cs`; do not edit by hand

/// ACE enum `TreasureType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreasureType(pub i32);

#[allow(non_upper_case_globals)]
impl TreasureType {
    pub const Undef: Self = Self(0);
    pub const Item: Self = Self(1);
    pub const MagicItem: Self = Self(2);
    pub const MundaneItem: Self = Self(3);
}

impl TreasureType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Item, Self::MagicItem, Self::MundaneItem];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Item", "MagicItem", "MundaneItem"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 0];
}

super::support::ace_enum!(TreasureType, i32, plain);
super::support::ace_enum_from!(TreasureType, i32 => i64);

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/TreasureTableType.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/TreasureTableType.cs`; do not edit by hand

/// ACE enum `TreasureTableType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreasureTableType(pub i32);

#[allow(non_upper_case_globals)]
impl TreasureTableType {
    pub const Undef: Self = Self(0);
    pub const ChanceInt: Self = Self(1);
    pub const ChanceSpell: Self = Self(2);
    pub const ChanceWcid: Self = Self(3);
    pub const ChanceBool: Self = Self(4);
    pub const ChanceGem: Self = Self(5);
    pub const ChanceHeritage: Self = Self(6);
    pub const ChanceItemType: Self = Self(7);
    pub const ChanceArmorType: Self = Self(8);
    pub const ChanceWeaponType: Self = Self(9);
}

impl TreasureTableType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::ChanceInt, Self::ChanceSpell, Self::ChanceWcid, Self::ChanceBool, Self::ChanceGem, Self::ChanceHeritage, Self::ChanceItemType, Self::ChanceArmorType, Self::ChanceWeaponType];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "ChanceInt", "ChanceSpell", "ChanceWcid", "ChanceBool", "ChanceGem", "ChanceHeritage", "ChanceItemType", "ChanceArmorType", "ChanceWeaponType"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[8, 4, 5, 6, 1, 7, 2, 3, 9, 0];
}

super::support::ace_enum!(TreasureTableType, i32, plain);
super::support::ace_enum_from!(TreasureTableType, i32 => i64);

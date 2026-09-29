// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/LootBias.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/LootBias.cs`; do not edit by hand

/// ACE enum `LootBias`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct LootBias(pub i32);

#[allow(non_upper_case_globals)]
impl LootBias {
    pub const UnBiased: Self = Self(0);
    pub const Armor: Self = Self(1);
    pub const Weapons: Self = Self(2);
    pub const SpellComps: Self = Self(3);
    pub const Clothing: Self = Self(4);
    pub const Jewelry: Self = Self(5);
    pub const MagicEquipment: Self = Self(6);
    pub const MixedEquipment: Self = Self(7);
}

impl LootBias {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::UnBiased, Self::Armor, Self::Weapons, Self::SpellComps, Self::Clothing, Self::Jewelry, Self::MagicEquipment, Self::MixedEquipment];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["UnBiased", "Armor", "Weapons", "SpellComps", "Clothing", "Jewelry", "MagicEquipment", "MixedEquipment"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 4, 5, 6, 7, 3, 0, 2];
}

super::support::ace_enum!(LootBias, i32, plain);
super::support::ace_enum_from!(LootBias, i32 => i64);

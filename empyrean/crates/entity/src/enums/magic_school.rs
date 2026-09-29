// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MagicSchool.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MagicSchool.cs`; do not edit by hand

/// ACE enum `MagicSchool`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MagicSchool(pub i32);

#[allow(non_upper_case_globals)]
impl MagicSchool {
    pub const None: Self = Self(0);
    pub const WarMagic: Self = Self(1);
    pub const LifeMagic: Self = Self(2);
    pub const ItemEnchantment: Self = Self(3);
    pub const CreatureEnchantment: Self = Self(4);
    pub const VoidMagic: Self = Self(5);
}

impl MagicSchool {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::WarMagic, Self::LifeMagic, Self::ItemEnchantment, Self::CreatureEnchantment, Self::VoidMagic];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "WarMagic", "LifeMagic", "ItemEnchantment", "CreatureEnchantment", "VoidMagic"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 3, 2, 0, 5, 1];
}

super::support::ace_enum!(MagicSchool, i32, plain);
super::support::ace_enum_from!(MagicSchool, i32 => i64);

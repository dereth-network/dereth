// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/MeleeWeaponSkill.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/MeleeWeaponSkill.cs`; do not edit by hand

/// ACE enum `MeleeWeaponSkill`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MeleeWeaponSkill(pub i32);

#[allow(non_upper_case_globals)]
impl MeleeWeaponSkill {
    pub const Undef: Self = Self(0);
    pub const HeavyWeapons: Self = Self(1);
    pub const LightWeapons: Self = Self(2);
    pub const FinesseWeapons: Self = Self(3);
    pub const TwoHandedCombat: Self = Self(4);
}

impl MeleeWeaponSkill {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::HeavyWeapons, Self::LightWeapons, Self::FinesseWeapons, Self::TwoHandedCombat];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "HeavyWeapons", "LightWeapons", "FinesseWeapons", "TwoHandedCombat"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 1, 2, 4, 0];
}

super::support::ace_enum!(MeleeWeaponSkill, i32, plain);
super::support::ace_enum_from!(MeleeWeaponSkill, i32 => i64);

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CombatUse.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CombatUse.cs`; do not edit by hand

/// ACE enum `CombatUse`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CombatUse(pub u8);

#[allow(non_upper_case_globals)]
impl CombatUse {
    pub const None: Self = Self(0);
    pub const Melee: Self = Self(1);
    pub const Missile: Self = Self(2);
    pub const Ammo: Self = Self(3);
    pub const Shield: Self = Self(4);
    pub const TwoHanded: Self = Self(5);
}

impl CombatUse {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Melee, Self::Missile, Self::Ammo, Self::Shield, Self::TwoHanded];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Melee", "Missile", "Ammo", "Shield", "TwoHanded"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 1, 2, 0, 4, 5];
}

super::support::ace_enum!(CombatUse, u8, plain);
super::support::ace_enum_from!(CombatUse, u8 => u16, u32, u64, i32, i64);

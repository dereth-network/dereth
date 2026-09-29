// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CombatMode.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CombatMode.cs`; do not edit by hand

/// ACE enum `CombatMode` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CombatMode(pub i32);

#[allow(non_upper_case_globals)]
impl CombatMode {
    pub const Undef: Self = Self(0x0);
    pub const NonCombat: Self = Self(0x1);
    pub const Melee: Self = Self(0x2);
    pub const Missile: Self = Self(0x4);
    pub const Magic: Self = Self(0x8);
    pub const ValidCombat: Self = Self(0xF);
    pub const CombatCombat: Self = Self(0xE);
}

impl CombatMode {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::NonCombat, Self::Melee, Self::Missile, Self::Magic, Self::CombatCombat, Self::ValidCombat];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "NonCombat", "Melee", "Missile", "Magic", "CombatCombat", "ValidCombat"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 4, 2, 3, 1, 0, 6];
}

super::support::ace_enum!(CombatMode, i32, flags);
super::support::ace_enum_from!(CombatMode, i32 => i64);

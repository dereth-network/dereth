// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ResistanceType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ResistanceType.cs`; do not edit by hand

/// ACE enum `ResistanceType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ResistanceType(pub i32);

#[allow(non_upper_case_globals)]
impl ResistanceType {
    pub const Undef: Self = Self(0);
    pub const Slash: Self = Self(1);
    pub const Pierce: Self = Self(2);
    pub const Bludgeon: Self = Self(3);
    pub const Fire: Self = Self(4);
    pub const Cold: Self = Self(5);
    pub const Acid: Self = Self(6);
    pub const Electric: Self = Self(7);
    pub const Nether: Self = Self(8);
    pub const HealthDrain: Self = Self(9);
    pub const HealthBoost: Self = Self(10);
    pub const StaminaDrain: Self = Self(11);
    pub const StaminaBoost: Self = Self(12);
    pub const ManaDrain: Self = Self(13);
    pub const ManaBoost: Self = Self(14);
}

impl ResistanceType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Slash, Self::Pierce, Self::Bludgeon, Self::Fire, Self::Cold, Self::Acid, Self::Electric, Self::Nether, Self::HealthDrain, Self::HealthBoost, Self::StaminaDrain, Self::StaminaBoost, Self::ManaDrain, Self::ManaBoost];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Slash", "Pierce", "Bludgeon", "Fire", "Cold", "Acid", "Electric", "Nether", "HealthDrain", "HealthBoost", "StaminaDrain", "StaminaBoost", "ManaDrain", "ManaBoost"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 3, 5, 7, 4, 10, 9, 14, 13, 8, 2, 1, 12, 11, 0];
}

super::support::ace_enum!(ResistanceType, i32, plain);
super::support::ace_enum_from!(ResistanceType, i32 => i64);

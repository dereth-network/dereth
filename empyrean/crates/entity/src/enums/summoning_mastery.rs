// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SummoningMastery.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SummoningMastery.cs`; do not edit by hand

/// ACE enum `SummoningMastery`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SummoningMastery(pub i32);

#[allow(non_upper_case_globals)]
impl SummoningMastery {
    pub const Undef: Self = Self(0);
    pub const Primalist: Self = Self(1);
    pub const Necromancer: Self = Self(2);
    pub const Naturalist: Self = Self(3);
}

impl SummoningMastery {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Primalist, Self::Necromancer, Self::Naturalist];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Primalist", "Necromancer", "Naturalist"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 1, 0];
}

super::support::ace_enum!(SummoningMastery, i32, plain);
super::support::ace_enum_from!(SummoningMastery, i32 => i64);

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AccessLevel.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AccessLevel.cs`; do not edit by hand

/// ACE enum `AccessLevel`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AccessLevel(pub i32);

#[allow(non_upper_case_globals)]
impl AccessLevel {
    pub const Player: Self = Self(0);
    pub const Advocate: Self = Self(1);
    pub const Sentinel: Self = Self(2);
    pub const Envoy: Self = Self(3);
    pub const Developer: Self = Self(4);
    pub const Admin: Self = Self(5);
}

impl AccessLevel {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Player, Self::Advocate, Self::Sentinel, Self::Envoy, Self::Developer, Self::Admin];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Player", "Advocate", "Sentinel", "Envoy", "Developer", "Admin"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 1, 4, 3, 0, 2];
}

super::support::ace_enum!(AccessLevel, i32, plain);
super::support::ace_enum_from!(AccessLevel, i32 => i64);

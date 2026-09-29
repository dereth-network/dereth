// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SecurityLevel.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SecurityLevel.cs`; do not edit by hand

/// ACE enum `SecurityLevel`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SecurityLevel(pub i32);

#[allow(non_upper_case_globals)]
impl SecurityLevel {
    pub const Undef: Self = Self(0);
    pub const Player: Self = Self(0);
    pub const Advocate1: Self = Self(1);
    pub const Advocate2: Self = Self(2);
    pub const Advocate3: Self = Self(3);
    pub const Advocate4: Self = Self(4);
    pub const Advocate5: Self = Self(5);
    pub const MaxAdvocate: Self = Self(5);
    pub const Sentinel1: Self = Self(6);
    pub const Sentinel2: Self = Self(7);
    pub const Sentinel3: Self = Self(8);
    pub const MaxSentinel: Self = Self(8);
    pub const Turbine: Self = Self(9);
    pub const Arch: Self = Self(10);
    pub const Admin: Self = Self(11);
    pub const Max: Self = Self(11);
}

impl SecurityLevel {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Player, Self::Advocate1, Self::Advocate2, Self::Advocate3, Self::Advocate4, Self::Advocate5, Self::MaxAdvocate, Self::Sentinel1, Self::Sentinel2, Self::Sentinel3, Self::MaxSentinel, Self::Turbine, Self::Arch, Self::Admin, Self::Max];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Player", "Advocate1", "Advocate2", "Advocate3", "Advocate4", "Advocate5", "MaxAdvocate", "Sentinel1", "Sentinel2", "Sentinel3", "MaxSentinel", "Turbine", "Arch", "Admin", "Max"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[14, 2, 3, 4, 5, 6, 13, 15, 7, 11, 1, 8, 9, 10, 12, 0];
}

super::support::ace_enum!(SecurityLevel, i32, plain);
super::support::ace_enum_from!(SecurityLevel, i32 => i64);

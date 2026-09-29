// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EmitterType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EmitterType.cs`; do not edit by hand

/// ACE enum `EmitterType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EmitterType(pub i32);

#[allow(non_upper_case_globals)]
impl EmitterType {
    pub const Unknown: Self = Self(0);
    pub const BirthratePerSec: Self = Self(1);
    pub const BirthratePerMeter: Self = Self(2);
}

impl EmitterType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Unknown, Self::BirthratePerSec, Self::BirthratePerMeter];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Unknown", "BirthratePerSec", "BirthratePerMeter"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(EmitterType, i32, plain);
super::support::ace_enum_from!(EmitterType, i32 => i64);

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PowerAccuracy.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PowerAccuracy.cs`; do not edit by hand

/// ACE enum `PowerAccuracy`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PowerAccuracy(pub i32);

#[allow(non_upper_case_globals)]
impl PowerAccuracy {
    pub const Low: Self = Self(1);
    pub const Medium: Self = Self(2);
    pub const High: Self = Self(3);
}

impl PowerAccuracy {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Low, Self::Medium, Self::High];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Low", "Medium", "High"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 0, 1];
}

super::support::ace_enum!(PowerAccuracy, i32, plain);
super::support::ace_enum_from!(PowerAccuracy, i32 => i64);

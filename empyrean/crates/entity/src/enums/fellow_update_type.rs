// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/FellowUpdateType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/FellowUpdateType.cs`; do not edit by hand

/// ACE enum `FellowUpdateType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct FellowUpdateType(pub i32);

#[allow(non_upper_case_globals)]
impl FellowUpdateType {
    pub const Undef: Self = Self(0);
    pub const Full: Self = Self(1);
    pub const Stats: Self = Self(2);
    pub const Vitals: Self = Self(3);
}

impl FellowUpdateType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Full, Self::Stats, Self::Vitals];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Full", "Stats", "Vitals"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0, 3];
}

super::support::ace_enum!(FellowUpdateType, i32, plain);
super::support::ace_enum_from!(FellowUpdateType, i32 => i64);

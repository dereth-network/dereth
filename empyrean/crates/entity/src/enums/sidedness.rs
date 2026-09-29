// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Sidedness.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Sidedness.cs`; do not edit by hand

/// ACE enum `Sidedness`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Sidedness(pub i32);

#[allow(non_upper_case_globals)]
impl Sidedness {
    pub const Positive: Self = Self(0);
    pub const Negative: Self = Self(1);
    pub const InPlane: Self = Self(2);
    pub const Crossing: Self = Self(3);
}

impl Sidedness {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Positive, Self::Negative, Self::InPlane, Self::Crossing];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Positive", "Negative", "InPlane", "Crossing"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 1, 0];
}

super::support::ace_enum!(Sidedness, i32, plain);
super::support::ace_enum_from!(Sidedness, i32 => i64);

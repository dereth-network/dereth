// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/DispelType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/DispelType.cs`; do not edit by hand

/// The type of spells to dispel
///
/// ACE enum `DispelType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct DispelType(pub i32);

#[allow(non_upper_case_globals)]
impl DispelType {
    pub const All: Self = Self(0);
    pub const Positive: Self = Self(1);
    pub const Negative: Self = Self(2);
}

impl DispelType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::All, Self::Positive, Self::Negative];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["All", "Positive", "Negative"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 2, 1];
}

super::support::ace_enum!(DispelType, i32, plain);
super::support::ace_enum_from!(DispelType, i32 => i64);

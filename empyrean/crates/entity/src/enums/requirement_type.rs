// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/RequirementType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/RequirementType.cs`; do not edit by hand

/// ACE enum `RequirementType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct RequirementType(pub i32);

#[allow(non_upper_case_globals)]
impl RequirementType {
    pub const Target: Self = Self(0);
    pub const Source: Self = Self(1);
    pub const Player: Self = Self(2);
}

impl RequirementType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Target, Self::Source, Self::Player];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Target", "Source", "Player"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(RequirementType, i32, plain);
super::support::ace_enum_from!(RequirementType, i32 => i64);

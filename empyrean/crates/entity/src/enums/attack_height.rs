// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/AttackHeight.cs
// @generated from ACE's `Source/ACE.Entity/Enum/AttackHeight.cs`; do not edit by hand

/// ACE enum `AttackHeight`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct AttackHeight(pub i32);

#[allow(non_upper_case_globals)]
impl AttackHeight {
    pub const High: Self = Self(1);
    pub const Medium: Self = Self(2);
    pub const Low: Self = Self(3);
}

impl AttackHeight {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::High, Self::Medium, Self::Low];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["High", "Medium", "Low"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 2, 1];
}

super::support::ace_enum!(AttackHeight, i32, plain);
super::support::ace_enum_from!(AttackHeight, i32 => i64);

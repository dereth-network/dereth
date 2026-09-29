// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PortalSummonType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PortalSummonType.cs`; do not edit by hand

/// ACE enum `PortalSummonType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PortalSummonType(pub i32);

#[allow(non_upper_case_globals)]
impl PortalSummonType {
    pub const Undef: Self = Self(0);
    pub const LinkedPortalOne: Self = Self(1);
    pub const LinkedPortalTwo: Self = Self(2);
}

impl PortalSummonType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::LinkedPortalOne, Self::LinkedPortalTwo];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "LinkedPortalOne", "LinkedPortalTwo"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(PortalSummonType, i32, plain);
super::support::ace_enum_from!(PortalSummonType, i32 => i64);

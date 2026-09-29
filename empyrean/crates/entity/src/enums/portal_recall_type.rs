// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PortalRecallType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PortalRecallType.cs`; do not edit by hand

/// ACE enum `PortalRecallType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PortalRecallType(pub i32);

#[allow(non_upper_case_globals)]
impl PortalRecallType {
    pub const Undef: Self = Self(0);
    pub const LastLifestone: Self = Self(1);
    pub const LinkedLifestone: Self = Self(2);
    pub const LastPortal: Self = Self(3);
    pub const LinkedPortalOne: Self = Self(4);
    pub const LinkedPortalTwo: Self = Self(5);
}

impl PortalRecallType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::LastLifestone, Self::LinkedLifestone, Self::LastPortal, Self::LinkedPortalOne, Self::LinkedPortalTwo];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "LastLifestone", "LinkedLifestone", "LastPortal", "LinkedPortalOne", "LinkedPortalTwo"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 3, 2, 4, 5, 0];
}

super::support::ace_enum!(PortalRecallType, i32, plain);
super::support::ace_enum_from!(PortalRecallType, i32 => i64);

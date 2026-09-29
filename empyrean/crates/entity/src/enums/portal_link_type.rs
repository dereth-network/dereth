// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PortalLinkType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PortalLinkType.cs`; do not edit by hand

/// ACE enum `PortalLinkType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PortalLinkType(pub i32);

#[allow(non_upper_case_globals)]
impl PortalLinkType {
    pub const Undef: Self = Self(0);
    pub const LinkedLifestone: Self = Self(1);
    pub const LinkedPortalOne: Self = Self(2);
    pub const LinkedPortalTwo: Self = Self(3);
}

impl PortalLinkType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::LinkedLifestone, Self::LinkedPortalOne, Self::LinkedPortalTwo];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "LinkedLifestone", "LinkedPortalOne", "LinkedPortalTwo"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 0];
}

super::support::ace_enum!(PortalLinkType, i32, plain);
super::support::ace_enum_from!(PortalLinkType, i32 => i64);

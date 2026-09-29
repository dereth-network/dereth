// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PortalFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PortalFlags.cs`; do not edit by hand

/// ACE enum `PortalFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PortalFlags(pub i32);

#[allow(non_upper_case_globals)]
impl PortalFlags {
    pub const ExactMatch: Self = Self(0x1);
    pub const PortalSide: Self = Self(0x2);
}

impl PortalFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::ExactMatch, Self::PortalSide];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["ExactMatch", "PortalSide"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 1];
}

super::support::ace_enum!(PortalFlags, i32, flags);
super::support::ace_enum_from!(PortalFlags, i32 => i64);

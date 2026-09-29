// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SetupFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SetupFlags.cs`; do not edit by hand

/// ACE enum `SetupFlags` (`[Flags]`), underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SetupFlags(pub i32);

#[allow(non_upper_case_globals)]
impl SetupFlags {
    pub const HasParent: Self = Self(0x1);
    pub const HasDefaultScale: Self = Self(0x2);
    pub const AllowFreeHeading: Self = Self(0x4);
    pub const HasPhysicsBSP: Self = Self(0x8);
}

impl SetupFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::HasParent, Self::HasDefaultScale, Self::AllowFreeHeading, Self::HasPhysicsBSP];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["HasParent", "HasDefaultScale", "AllowFreeHeading", "HasPhysicsBSP"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0, 3];
}

super::support::ace_enum!(SetupFlags, i32, flags);
super::support::ace_enum_from!(SetupFlags, i32 => i64);

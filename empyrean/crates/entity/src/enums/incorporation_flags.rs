// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/IncorporationFlags.cs
// @generated from ACE's `Source/ACE.Entity/Enum/IncorporationFlags.cs`; do not edit by hand

/// ACE enum `IncorporationFlags`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct IncorporationFlags(pub i32);

#[allow(non_upper_case_globals)]
impl IncorporationFlags {
    pub const PassToChildren: Self = Self(1);
    pub const X: Self = Self(2);
    pub const Y: Self = Self(4);
    pub const Width: Self = Self(8);
    pub const Height: Self = Self(16);
    pub const ZLevel: Self = Self(32);
}

impl IncorporationFlags {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::PassToChildren, Self::X, Self::Y, Self::Width, Self::Height, Self::ZLevel];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["PassToChildren", "X", "Y", "Width", "Height", "ZLevel"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 0, 3, 1, 2, 5];
}

super::support::ace_enum!(IncorporationFlags, i32, plain);
super::support::ace_enum_from!(IncorporationFlags, i32 => i64);

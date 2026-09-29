// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/SurfaceHandler.cs
// @generated from ACE's `Source/ACE.Entity/Enum/SurfaceHandler.cs`; do not edit by hand

/// ACE enum `SurfaceHandler`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SurfaceHandler(pub i32);

#[allow(non_upper_case_globals)]
impl SurfaceHandler {
    pub const Invalid: Self = Self(0);
    pub const Database: Self = Self(1);
    pub const PalShift: Self = Self(2);
    pub const TexMerge: Self = Self(3);
    pub const CustomDB: Self = Self(4);
}

impl SurfaceHandler {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Database, Self::PalShift, Self::TexMerge, Self::CustomDB];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Database", "PalShift", "TexMerge", "CustomDB"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 0, 2, 3];
}

super::support::ace_enum!(SurfaceHandler, i32, plain);
super::support::ace_enum_from!(SurfaceHandler, i32 => i64);

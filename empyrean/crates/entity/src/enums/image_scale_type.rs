// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ImageScaleType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ImageScaleType.cs`; do not edit by hand

/// ACE enum `ImageScaleType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ImageScaleType(pub i32);

#[allow(non_upper_case_globals)]
impl ImageScaleType {
    pub const Full: Self = Self(0);
    pub const Half: Self = Self(1);
    pub const Quarter: Self = Self(2);
    pub const Eighth: Self = Self(4);
}

impl ImageScaleType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Full, Self::Half, Self::Quarter, Self::Eighth];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Full", "Half", "Quarter", "Eighth"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 0, 1, 2];
}

super::support::ace_enum!(ImageScaleType, i32, plain);
super::support::ace_enum_from!(ImageScaleType, i32 => i64);

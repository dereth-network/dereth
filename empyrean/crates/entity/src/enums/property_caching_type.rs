// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PropertyCachingType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PropertyCachingType.cs`; do not edit by hand

/// ACE enum `PropertyCachingType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PropertyCachingType(pub i32);

#[allow(non_upper_case_globals)]
impl PropertyCachingType {
    pub const Global: Self = Self(0);
    pub const Internal: Self = Self(1);
}

impl PropertyCachingType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Global, Self::Internal];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Global", "Internal"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 1];
}

super::support::ace_enum!(PropertyCachingType, i32, plain);
super::support::ace_enum_from!(PropertyCachingType, i32 => i64);

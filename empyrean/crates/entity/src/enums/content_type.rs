// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ContentType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ContentType.cs`; do not edit by hand

/// ACE enum `ContentType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ContentType(pub i32);

#[allow(non_upper_case_globals)]
impl ContentType {
    pub const Patch: Self = Self(1);
    pub const Quest: Self = Self(2);
}

impl ContentType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Patch, Self::Quest];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Patch", "Quest"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[0, 1];
}

super::support::ace_enum!(ContentType, i32, plain);
super::support::ace_enum_from!(ContentType, i32 => i64);

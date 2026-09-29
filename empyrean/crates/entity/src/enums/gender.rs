// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/Gender.cs
// @generated from ACE's `Source/ACE.Entity/Enum/Gender.cs`; do not edit by hand

/// ACE enum `Gender`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct Gender(pub i32);

#[allow(non_upper_case_globals)]
impl Gender {
    pub const Invalid: Self = Self(0);
    pub const Male: Self = Self(1);
    pub const Female: Self = Self(2);
}

impl Gender {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Male, Self::Female];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Male", "Female"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 0, 1];
}

super::support::ace_enum!(Gender, i32, plain);
super::support::ace_enum_from!(Gender, i32 => i64);

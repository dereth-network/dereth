// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GeneratorType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GeneratorType.cs`; do not edit by hand

/// ACE enum `GeneratorType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GeneratorType(pub i32);

#[allow(non_upper_case_globals)]
impl GeneratorType {
    pub const Undef: Self = Self(0);
    pub const Relative: Self = Self(1);
    pub const Absolute: Self = Self(2);
}

impl GeneratorType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Relative, Self::Absolute];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Relative", "Absolute"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(GeneratorType, i32, plain);
super::support::ace_enum_from!(GeneratorType, i32 => i64);

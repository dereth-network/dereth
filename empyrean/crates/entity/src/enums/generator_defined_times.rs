// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GeneratorDefinedTimes.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GeneratorDefinedTimes.cs`; do not edit by hand

/// ACE enum `GeneratorDefinedTimes`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GeneratorDefinedTimes(pub i32);

#[allow(non_upper_case_globals)]
impl GeneratorDefinedTimes {
    pub const Undef: Self = Self(0);
    pub const Dusk: Self = Self(1);
    pub const Dawn: Self = Self(2);
}

impl GeneratorDefinedTimes {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Dusk, Self::Dawn];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Dusk", "Dawn"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[2, 1, 0];
}

super::support::ace_enum!(GeneratorDefinedTimes, i32, plain);
super::support::ace_enum_from!(GeneratorDefinedTimes, i32 => i64);

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/GeneratorTimeType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/GeneratorTimeType.cs`; do not edit by hand

/// ACE enum `GeneratorTimeType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct GeneratorTimeType(pub i32);

#[allow(non_upper_case_globals)]
impl GeneratorTimeType {
    pub const Undef: Self = Self(0);
    pub const RealTime: Self = Self(1);
    pub const Defined: Self = Self(2);
    pub const Event: Self = Self(3);
    pub const Night: Self = Self(4);
    pub const Day: Self = Self(5);
}

impl GeneratorTimeType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::RealTime, Self::Defined, Self::Event, Self::Night, Self::Day];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "RealTime", "Defined", "Event", "Night", "Day"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 2, 3, 4, 1, 0];
}

super::support::ace_enum!(GeneratorTimeType, i32, plain);
super::support::ace_enum_from!(GeneratorTimeType, i32 => i64);

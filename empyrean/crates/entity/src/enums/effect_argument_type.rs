// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EffectArgumentType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EffectArgumentType.cs`; do not edit by hand

/// ACE enum `EffectArgumentType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EffectArgumentType(pub i32);

#[allow(non_upper_case_globals)]
impl EffectArgumentType {
    pub const Invalid: Self = Self(0);
    pub const Double: Self = Self(1);
    pub const Int: Self = Self(2);
    pub const Quality: Self = Self(3);
    pub const Random: Self = Self(4);
    pub const Variable: Self = Self(5);
    pub const Int64: Self = Self(6);
}

impl EffectArgumentType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Double, Self::Int, Self::Quality, Self::Random, Self::Variable, Self::Int64];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Double", "Int", "Quality", "Random", "Variable", "Int64"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 6, 0, 3, 4, 5];
}

super::support::ace_enum!(EffectArgumentType, i32, plain);
super::support::ace_enum_from!(EffectArgumentType, i32 => i64);

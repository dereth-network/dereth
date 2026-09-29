// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/NumberingType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/NumberingType.cs`; do not edit by hand

/// ACE enum `NumberingType`, underlying `byte`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct NumberingType(pub u8);

#[allow(non_upper_case_globals)]
impl NumberingType {
    pub const Undefined: Self = Self(0);
    pub const Normal: Self = Self(1);
    pub const Sequential: Self = Self(1);
    pub const Bitfield: Self = Self(2);
    pub const Bitfield32: Self = Self(3);
    pub const Bitfield64: Self = Self(4);
}

impl NumberingType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undefined, Self::Normal, Self::Sequential, Self::Bitfield, Self::Bitfield32, Self::Bitfield64];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undefined", "Normal", "Sequential", "Bitfield", "Bitfield32", "Bitfield64"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 4, 5, 1, 2, 0];
}

super::support::ace_enum!(NumberingType, u8, plain);
super::support::ace_enum_from!(NumberingType, u8 => u16, u32, u64, i32, i64);

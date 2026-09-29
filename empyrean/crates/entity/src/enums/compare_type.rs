// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CompareType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CompareType.cs`; do not edit by hand

/// ACE enum `CompareType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CompareType(pub i32);

#[allow(non_upper_case_globals)]
impl CompareType {
    pub const GreaterThan: Self = Self(0);
    pub const LessThanEqual: Self = Self(1);
    pub const LessThan: Self = Self(2);
    pub const GreaterThanEqual: Self = Self(3);
    pub const NotEqual: Self = Self(4);
    pub const NotEqualNotExist: Self = Self(5);
    pub const Equal: Self = Self(6);
    pub const NotExist: Self = Self(7);
    pub const Exist: Self = Self(8);
    pub const NotHasBits: Self = Self(9);
    pub const HasBits: Self = Self(10);
}

impl CompareType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::GreaterThan, Self::LessThanEqual, Self::LessThan, Self::GreaterThanEqual, Self::NotEqual, Self::NotEqualNotExist, Self::Equal, Self::NotExist, Self::Exist, Self::NotHasBits, Self::HasBits];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["GreaterThan", "LessThanEqual", "LessThan", "GreaterThanEqual", "NotEqual", "NotEqualNotExist", "Equal", "NotExist", "Exist", "NotHasBits", "HasBits"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 8, 0, 3, 10, 2, 1, 4, 5, 7, 9];
}

super::support::ace_enum!(CompareType, i32, plain);
super::support::ace_enum_from!(CompareType, i32 => i64);

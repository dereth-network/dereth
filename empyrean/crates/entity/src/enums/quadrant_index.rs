// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/QuadrantIndex.cs
// @generated from ACE's `Source/ACE.Entity/Enum/QuadrantIndex.cs`; do not edit by hand

/// ACE enum `QuadrantIndex`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct QuadrantIndex(pub i32);

#[allow(non_upper_case_globals)]
impl QuadrantIndex {
    pub const HLF: Self = Self(0);
    pub const MLF: Self = Self(1);
    pub const LLF: Self = Self(2);
    pub const HRF: Self = Self(3);
    pub const MRF: Self = Self(4);
    pub const LRF: Self = Self(5);
    pub const HLB: Self = Self(6);
    pub const MLB: Self = Self(7);
    pub const LLB: Self = Self(8);
    pub const HRB: Self = Self(9);
    pub const MRB: Self = Self(10);
    pub const LRB: Self = Self(11);
}

impl QuadrantIndex {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::HLF, Self::MLF, Self::LLF, Self::HRF, Self::MRF, Self::LRF, Self::HLB, Self::MLB, Self::LLB, Self::HRB, Self::MRB, Self::LRB];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["HLF", "MLF", "LLF", "HRF", "MRF", "LRF", "HLB", "MLB", "LLB", "HRB", "MRB", "LRB"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[6, 0, 9, 3, 8, 2, 11, 5, 7, 1, 10, 4];
}

super::support::ace_enum!(QuadrantIndex, i32, plain);
super::support::ace_enum_from!(QuadrantIndex, i32 => i64);

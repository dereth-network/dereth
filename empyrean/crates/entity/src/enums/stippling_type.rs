// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/StipplingType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/StipplingType.cs`; do not edit by hand

/// ACE enum `StipplingType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct StipplingType(pub i32);

#[allow(non_upper_case_globals)]
impl StipplingType {
    pub const None: Self = Self(0);
    pub const Positive: Self = Self(1);
    pub const Negative: Self = Self(2);
    pub const Both: Self = Self(3);
    pub const NoPos: Self = Self(4);
    pub const NoNeg: Self = Self(8);
    pub const NoUVS: Self = Self(20);
}

impl StipplingType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Positive, Self::Negative, Self::Both, Self::NoPos, Self::NoNeg, Self::NoUVS];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Positive", "Negative", "Both", "NoPos", "NoNeg", "NoUVS"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[3, 2, 5, 4, 6, 0, 1];
}

super::support::ace_enum!(StipplingType, i32, plain);
super::support::ace_enum_from!(StipplingType, i32 => i64);

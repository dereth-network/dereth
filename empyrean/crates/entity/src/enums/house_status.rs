// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HouseStatus.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HouseStatus.cs`; do not edit by hand

/// ACE enum `HouseStatus`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HouseStatus(pub i32);

#[allow(non_upper_case_globals)]
impl HouseStatus {
    pub const Disabled: Self = Self(-1);
    pub const InActive: Self = Self(0);
    pub const Active: Self = Self(1);
}

impl HouseStatus {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::InActive, Self::Active, Self::Disabled];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["InActive", "Active", "Disabled"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 0];
}

super::support::ace_enum!(HouseStatus, i32, plain);
super::support::ace_enum_from!(HouseStatus, i32 => i64);

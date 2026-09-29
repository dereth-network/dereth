// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/HouseType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/HouseType.cs`; do not edit by hand

/// ACE enum `HouseType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct HouseType(pub i32);

#[allow(non_upper_case_globals)]
impl HouseType {
    pub const Undef: Self = Self(0);
    pub const Cottage: Self = Self(1);
    pub const Villa: Self = Self(2);
    pub const Mansion: Self = Self(3);
    pub const Apartment: Self = Self(4);
}

impl HouseType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Cottage, Self::Villa, Self::Mansion, Self::Apartment];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Cottage", "Villa", "Mansion", "Apartment"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[4, 1, 3, 0, 2];
}

super::support::ace_enum!(HouseType, i32, plain);
super::support::ace_enum_from!(HouseType, i32 => i64);

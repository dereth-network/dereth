// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/ArmorType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/ArmorType.cs`; do not edit by hand

/// ACE enum `ArmorType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct ArmorType(pub i32);

#[allow(non_upper_case_globals)]
impl ArmorType {
    pub const None: Self = Self(0);
    pub const Cloth: Self = Self(1);
    pub const Leather: Self = Self(2);
    pub const StuddedLeather: Self = Self(4);
    pub const Scalemail: Self = Self(8);
    pub const Chainmail: Self = Self(16);
    pub const Metal: Self = Self(32);
}

impl ArmorType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::None, Self::Cloth, Self::Leather, Self::StuddedLeather, Self::Scalemail, Self::Chainmail, Self::Metal];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["None", "Cloth", "Leather", "StuddedLeather", "Scalemail", "Chainmail", "Metal"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[5, 1, 2, 6, 0, 4, 3];
}

super::support::ace_enum!(ArmorType, i32, plain);
super::support::ace_enum_from!(ArmorType, i32 => i64);

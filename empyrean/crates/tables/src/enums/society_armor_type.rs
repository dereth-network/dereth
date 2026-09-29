// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/SocietyArmorType.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/SocietyArmorType.cs`; do not edit by hand

/// ACE enum `SocietyArmorType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct SocietyArmorType(pub i32);

#[allow(non_upper_case_globals)]
impl SocietyArmorType {
    pub const Undef: Self = Self(0);
    pub const Breastplate: Self = Self(1);
    pub const Gauntlets: Self = Self(2);
    pub const Girth: Self = Self(3);
    pub const Greaves: Self = Self(4);
    pub const Helm: Self = Self(5);
    pub const Pauldrons: Self = Self(6);
    pub const Tassets: Self = Self(7);
    pub const Vambraces: Self = Self(8);
    pub const Sollerets: Self = Self(9);
}

impl SocietyArmorType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Breastplate, Self::Gauntlets, Self::Girth, Self::Greaves, Self::Helm, Self::Pauldrons, Self::Tassets, Self::Vambraces, Self::Sollerets];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Breastplate", "Gauntlets", "Girth", "Greaves", "Helm", "Pauldrons", "Tassets", "Vambraces", "Sollerets"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[1, 2, 3, 4, 5, 6, 9, 7, 0, 8];
}

super::support::ace_enum!(SocietyArmorType, i32, plain);
super::support::ace_enum_from!(SocietyArmorType, i32 => i64);

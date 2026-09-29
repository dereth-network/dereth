// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Enum/TreasureArmorType.cs
// @generated from ACE's `Source/ACE.Server/Factories/Enum/TreasureArmorType.cs`; do not edit by hand

/// ACE enum `TreasureArmorType`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct TreasureArmorType(pub i32);

#[allow(non_upper_case_globals)]
impl TreasureArmorType {
    pub const Undef: Self = Self(0);
    pub const Leather: Self = Self(1);
    pub const StuddedLeather: Self = Self(2);
    pub const Chainmail: Self = Self(3);
    pub const Platemail: Self = Self(4);
    pub const Scalemail: Self = Self(5);
    pub const Yoroi: Self = Self(6);
    pub const HeritageLow: Self = Self(7);
    pub const Celdon: Self = Self(8);
    pub const Amuli: Self = Self(9);
    pub const Koujia: Self = Self(10);
    pub const Covenant: Self = Self(11);
    pub const HeritageHigh: Self = Self(12);
    pub const Lorica: Self = Self(13);
    pub const Nariyid: Self = Self(14);
    pub const Chiran: Self = Self(15);
    pub const Diforsa: Self = Self(16);
    pub const Tenassa: Self = Self(17);
    pub const Alduressa: Self = Self(18);
    pub const Olthoi: Self = Self(19);
    pub const OlthoiHeritage: Self = Self(20);
    pub const OlthoiCeldon: Self = Self(21);
    pub const OlthoiAmuli: Self = Self(22);
    pub const OlthoiKoujia: Self = Self(23);
    pub const OlthoiAlduressa: Self = Self(24);
    pub const Society: Self = Self(25);
    pub const CelestialHand: Self = Self(26);
    pub const EldrytchWeb: Self = Self(27);
    pub const RadiantBlood: Self = Self(28);
    pub const Haebrean: Self = Self(29);
    pub const KnorrAcademy: Self = Self(30);
    pub const Sedgemail: Self = Self(31);
    pub const Overrobe: Self = Self(32);
}

impl TreasureArmorType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::Leather, Self::StuddedLeather, Self::Chainmail, Self::Platemail, Self::Scalemail, Self::Yoroi, Self::HeritageLow, Self::Celdon, Self::Amuli, Self::Koujia, Self::Covenant, Self::HeritageHigh, Self::Lorica, Self::Nariyid, Self::Chiran, Self::Diforsa, Self::Tenassa, Self::Alduressa, Self::Olthoi, Self::OlthoiHeritage, Self::OlthoiCeldon, Self::OlthoiAmuli, Self::OlthoiKoujia, Self::OlthoiAlduressa, Self::Society, Self::CelestialHand, Self::EldrytchWeb, Self::RadiantBlood, Self::Haebrean, Self::KnorrAcademy, Self::Sedgemail, Self::Overrobe];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "Leather", "StuddedLeather", "Chainmail", "Platemail", "Scalemail", "Yoroi", "HeritageLow", "Celdon", "Amuli", "Koujia", "Covenant", "HeritageHigh", "Lorica", "Nariyid", "Chiran", "Diforsa", "Tenassa", "Alduressa", "Olthoi", "OlthoiHeritage", "OlthoiCeldon", "OlthoiAmuli", "OlthoiKoujia", "OlthoiAlduressa", "Society", "CelestialHand", "EldrytchWeb", "RadiantBlood", "Haebrean", "KnorrAcademy", "Sedgemail", "Overrobe"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[18, 9, 8, 26, 3, 15, 11, 16, 27, 29, 12, 7, 30, 10, 1, 13, 14, 19, 24, 22, 21, 20, 23, 32, 4, 28, 5, 31, 25, 2, 17, 0, 6];
}

super::support::ace_enum!(TreasureArmorType, i32, plain);
super::support::ace_enum_from!(TreasureArmorType, i32 => i64);

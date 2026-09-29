// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/MaterialType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/MaterialType.cs`; do not edit by hand

/// ACE enum `MaterialType`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct MaterialType(pub u32);

#[allow(non_upper_case_globals)]
impl MaterialType {
    pub const Unknown: Self = Self(0);
    pub const Ceramic: Self = Self(1);
    pub const Porcelain: Self = Self(2);
    pub const Cloth: Self = Self(3);
    pub const Linen: Self = Self(4);
    pub const Satin: Self = Self(5);
    pub const Silk: Self = Self(6);
    pub const Velvet: Self = Self(7);
    pub const Wool: Self = Self(8);
    pub const Gem: Self = Self(9);
    pub const Agate: Self = Self(10);
    pub const Amber: Self = Self(11);
    pub const Amethyst: Self = Self(12);
    pub const Aquamarine: Self = Self(13);
    pub const Azurite: Self = Self(14);
    pub const BlackGarnet: Self = Self(15);
    pub const BlackOpal: Self = Self(16);
    pub const Bloodstone: Self = Self(17);
    pub const Carnelian: Self = Self(18);
    pub const Citrine: Self = Self(19);
    pub const Diamond: Self = Self(20);
    pub const Emerald: Self = Self(21);
    pub const FireOpal: Self = Self(22);
    pub const GreenGarnet: Self = Self(23);
    pub const GreenJade: Self = Self(24);
    pub const Hematite: Self = Self(25);
    pub const ImperialTopaz: Self = Self(26);
    pub const Jet: Self = Self(27);
    pub const LapisLazuli: Self = Self(28);
    pub const LavenderJade: Self = Self(29);
    pub const Malachite: Self = Self(30);
    pub const Moonstone: Self = Self(31);
    pub const Onyx: Self = Self(32);
    pub const Opal: Self = Self(33);
    pub const Peridot: Self = Self(34);
    pub const RedGarnet: Self = Self(35);
    pub const RedJade: Self = Self(36);
    pub const RoseQuartz: Self = Self(37);
    pub const Ruby: Self = Self(38);
    pub const Sapphire: Self = Self(39);
    pub const SmokeyQuartz: Self = Self(40);
    pub const Sunstone: Self = Self(41);
    pub const TigerEye: Self = Self(42);
    pub const Tourmaline: Self = Self(43);
    pub const Turquoise: Self = Self(44);
    pub const WhiteJade: Self = Self(45);
    pub const WhiteQuartz: Self = Self(46);
    pub const WhiteSapphire: Self = Self(47);
    pub const YellowGarnet: Self = Self(48);
    pub const YellowTopaz: Self = Self(49);
    pub const Zircon: Self = Self(50);
    pub const Ivory: Self = Self(51);
    pub const Leather: Self = Self(52);
    pub const ArmoredilloHide: Self = Self(53);
    pub const GromnieHide: Self = Self(54);
    pub const ReedSharkHide: Self = Self(55);
    pub const Metal: Self = Self(56);
    pub const Brass: Self = Self(57);
    pub const Bronze: Self = Self(58);
    pub const Copper: Self = Self(59);
    pub const Gold: Self = Self(60);
    pub const Iron: Self = Self(61);
    pub const Pyreal: Self = Self(62);
    pub const Silver: Self = Self(63);
    pub const Steel: Self = Self(64);
    pub const Stone: Self = Self(65);
    pub const Alabaster: Self = Self(66);
    pub const Granite: Self = Self(67);
    pub const Marble: Self = Self(68);
    pub const Obsidian: Self = Self(69);
    pub const Sandstone: Self = Self(70);
    pub const Serpentine: Self = Self(71);
    pub const Wood: Self = Self(72);
    pub const Ebony: Self = Self(73);
    pub const Mahogany: Self = Self(74);
    pub const Oak: Self = Self(75);
    pub const Pine: Self = Self(76);
    pub const Teak: Self = Self(77);
}

impl MaterialType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Unknown, Self::Ceramic, Self::Porcelain, Self::Cloth, Self::Linen, Self::Satin, Self::Silk, Self::Velvet, Self::Wool, Self::Gem, Self::Agate, Self::Amber, Self::Amethyst, Self::Aquamarine, Self::Azurite, Self::BlackGarnet, Self::BlackOpal, Self::Bloodstone, Self::Carnelian, Self::Citrine, Self::Diamond, Self::Emerald, Self::FireOpal, Self::GreenGarnet, Self::GreenJade, Self::Hematite, Self::ImperialTopaz, Self::Jet, Self::LapisLazuli, Self::LavenderJade, Self::Malachite, Self::Moonstone, Self::Onyx, Self::Opal, Self::Peridot, Self::RedGarnet, Self::RedJade, Self::RoseQuartz, Self::Ruby, Self::Sapphire, Self::SmokeyQuartz, Self::Sunstone, Self::TigerEye, Self::Tourmaline, Self::Turquoise, Self::WhiteJade, Self::WhiteQuartz, Self::WhiteSapphire, Self::YellowGarnet, Self::YellowTopaz, Self::Zircon, Self::Ivory, Self::Leather, Self::ArmoredilloHide, Self::GromnieHide, Self::ReedSharkHide, Self::Metal, Self::Brass, Self::Bronze, Self::Copper, Self::Gold, Self::Iron, Self::Pyreal, Self::Silver, Self::Steel, Self::Stone, Self::Alabaster, Self::Granite, Self::Marble, Self::Obsidian, Self::Sandstone, Self::Serpentine, Self::Wood, Self::Ebony, Self::Mahogany, Self::Oak, Self::Pine, Self::Teak];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Unknown", "Ceramic", "Porcelain", "Cloth", "Linen", "Satin", "Silk", "Velvet", "Wool", "Gem", "Agate", "Amber", "Amethyst", "Aquamarine", "Azurite", "BlackGarnet", "BlackOpal", "Bloodstone", "Carnelian", "Citrine", "Diamond", "Emerald", "FireOpal", "GreenGarnet", "GreenJade", "Hematite", "ImperialTopaz", "Jet", "LapisLazuli", "LavenderJade", "Malachite", "Moonstone", "Onyx", "Opal", "Peridot", "RedGarnet", "RedJade", "RoseQuartz", "Ruby", "Sapphire", "SmokeyQuartz", "Sunstone", "TigerEye", "Tourmaline", "Turquoise", "WhiteJade", "WhiteQuartz", "WhiteSapphire", "YellowGarnet", "YellowTopaz", "Zircon", "Ivory", "Leather", "ArmoredilloHide", "GromnieHide", "ReedSharkHide", "Metal", "Brass", "Bronze", "Copper", "Gold", "Iron", "Pyreal", "Silver", "Steel", "Stone", "Alabaster", "Granite", "Marble", "Obsidian", "Sandstone", "Serpentine", "Wood", "Ebony", "Mahogany", "Oak", "Pine", "Teak"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[10, 66, 11, 12, 13, 53, 14, 15, 16, 17, 57, 58, 18, 1, 19, 3, 59, 20, 73, 21, 22, 9, 60, 67, 23, 24, 54, 25, 26, 61, 51, 27, 28, 29, 52, 4, 74, 30, 68, 56, 31, 75, 69, 32, 33, 34, 76, 2, 62, 35, 36, 55, 37, 38, 70, 39, 5, 71, 6, 63, 40, 64, 65, 41, 77, 42, 43, 44, 0, 7, 45, 46, 47, 72, 8, 48, 49, 50];
}

super::support::ace_enum!(MaterialType, u32, plain);
super::support::ace_enum_from!(MaterialType, u32 => u64, i64);

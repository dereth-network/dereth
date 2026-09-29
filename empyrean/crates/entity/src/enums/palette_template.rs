// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/PaletteTemplate.cs
// @generated from ACE's `Source/ACE.Entity/Enum/PaletteTemplate.cs`; do not edit by hand

/// ACE enum `PaletteTemplate`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct PaletteTemplate(pub i32);

#[allow(non_upper_case_globals)]
impl PaletteTemplate {
    pub const Undef: Self = Self(0);
    pub const AquaBlue: Self = Self(1);
    pub const Blue: Self = Self(2);
    pub const BluePurple: Self = Self(3);
    pub const Brown: Self = Self(4);
    pub const DarkBlue: Self = Self(5);
    pub const DeepBrown: Self = Self(6);
    pub const DeepGreen: Self = Self(7);
    pub const Green: Self = Self(8);
    pub const Grey: Self = Self(9);
    pub const LightBlue: Self = Self(10);
    pub const Maroon: Self = Self(11);
    pub const Navy: Self = Self(12);
    pub const Purple: Self = Self(13);
    pub const Red: Self = Self(14);
    pub const RedPurple: Self = Self(15);
    pub const Rose: Self = Self(16);
    pub const Yellow: Self = Self(17);
    pub const YellowBrown: Self = Self(18);
    pub const Copper: Self = Self(19);
    pub const Silver: Self = Self(20);
    pub const Gold: Self = Self(21);
    pub const Aqua: Self = Self(22);
    pub const DarkAquaMetal: Self = Self(23);
    pub const DarkBlueMetal: Self = Self(24);
    pub const DarkCopperMetal: Self = Self(25);
    pub const DarkGoldMetal: Self = Self(26);
    pub const DarkGreenMetal: Self = Self(27);
    pub const DarkPurpleMetal: Self = Self(28);
    pub const DarkRedMetal: Self = Self(29);
    pub const DarkSilverMetal: Self = Self(30);
    pub const LightAquaMetal: Self = Self(31);
    pub const LightBlueMetal: Self = Self(32);
    pub const LightCopperMetal: Self = Self(33);
    pub const LightGoldMetal: Self = Self(34);
    pub const LightGreenMetal: Self = Self(35);
    pub const LightPurpleMetal: Self = Self(36);
    pub const LightRedMetal: Self = Self(37);
    pub const LightSilverMetal: Self = Self(38);
    pub const Black: Self = Self(39);
    pub const Bronze: Self = Self(40);
    pub const SandyYellow: Self = Self(41);
    pub const DarkBrown: Self = Self(42);
    pub const LightBrown: Self = Self(43);
    pub const TanRed: Self = Self(44);
    pub const PaleGreen: Self = Self(45);
    pub const Tan: Self = Self(46);
    pub const PastyYellow: Self = Self(47);
    pub const SnowyWhite: Self = Self(48);
    pub const RuddyYellow: Self = Self(49);
    pub const RuddierYellow: Self = Self(50);
    pub const MidGrey: Self = Self(51);
    pub const DarkGrey: Self = Self(52);
    pub const BlueDullSilver: Self = Self(53);
    pub const YellowPaleSilver: Self = Self(54);
    pub const BrownBlueDark: Self = Self(55);
    pub const BrownBlueMed: Self = Self(56);
    pub const GreenSilver: Self = Self(57);
    pub const BrownGreen: Self = Self(58);
    pub const YellowGreen: Self = Self(59);
    pub const PalePurple: Self = Self(60);
    pub const White: Self = Self(61);
    pub const RedBrown: Self = Self(62);
    pub const GreenBrown: Self = Self(63);
    pub const OrangeBrown: Self = Self(64);
    pub const PaleGreenBrown: Self = Self(65);
    pub const PaleOrange: Self = Self(66);
    pub const GreenSlime: Self = Self(67);
    pub const BlueSlime: Self = Self(68);
    pub const YellowSlime: Self = Self(69);
    pub const PurpleSlime: Self = Self(70);
    pub const DullRed: Self = Self(71);
    pub const GreyWhite: Self = Self(72);
    pub const MediumGrey: Self = Self(73);
    pub const DullGreen: Self = Self(74);
    pub const OliveGreen: Self = Self(75);
    pub const Orange: Self = Self(76);
    pub const BlueGreen: Self = Self(77);
    pub const Olive: Self = Self(78);
    pub const Lead: Self = Self(79);
    pub const Iron: Self = Self(80);
    pub const LiteGreen: Self = Self(81);
    pub const PinkPurple: Self = Self(82);
    pub const Amber: Self = Self(83);
    pub const DyeDarkGreen: Self = Self(84);
    pub const DyeDarkRed: Self = Self(85);
    pub const DyeDarkYellow: Self = Self(86);
    pub const DyeBotched: Self = Self(87);
    pub const DyeWinterBlue: Self = Self(88);
    pub const DyeWinterGreen: Self = Self(89);
    pub const DyeWinterSilver: Self = Self(90);
    pub const DyeSpringBlue: Self = Self(91);
    pub const DyeSpringPurple: Self = Self(92);
    pub const DyeSpringBlack: Self = Self(93);
}

impl PaletteTemplate {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Undef, Self::AquaBlue, Self::Blue, Self::BluePurple, Self::Brown, Self::DarkBlue, Self::DeepBrown, Self::DeepGreen, Self::Green, Self::Grey, Self::LightBlue, Self::Maroon, Self::Navy, Self::Purple, Self::Red, Self::RedPurple, Self::Rose, Self::Yellow, Self::YellowBrown, Self::Copper, Self::Silver, Self::Gold, Self::Aqua, Self::DarkAquaMetal, Self::DarkBlueMetal, Self::DarkCopperMetal, Self::DarkGoldMetal, Self::DarkGreenMetal, Self::DarkPurpleMetal, Self::DarkRedMetal, Self::DarkSilverMetal, Self::LightAquaMetal, Self::LightBlueMetal, Self::LightCopperMetal, Self::LightGoldMetal, Self::LightGreenMetal, Self::LightPurpleMetal, Self::LightRedMetal, Self::LightSilverMetal, Self::Black, Self::Bronze, Self::SandyYellow, Self::DarkBrown, Self::LightBrown, Self::TanRed, Self::PaleGreen, Self::Tan, Self::PastyYellow, Self::SnowyWhite, Self::RuddyYellow, Self::RuddierYellow, Self::MidGrey, Self::DarkGrey, Self::BlueDullSilver, Self::YellowPaleSilver, Self::BrownBlueDark, Self::BrownBlueMed, Self::GreenSilver, Self::BrownGreen, Self::YellowGreen, Self::PalePurple, Self::White, Self::RedBrown, Self::GreenBrown, Self::OrangeBrown, Self::PaleGreenBrown, Self::PaleOrange, Self::GreenSlime, Self::BlueSlime, Self::YellowSlime, Self::PurpleSlime, Self::DullRed, Self::GreyWhite, Self::MediumGrey, Self::DullGreen, Self::OliveGreen, Self::Orange, Self::BlueGreen, Self::Olive, Self::Lead, Self::Iron, Self::LiteGreen, Self::PinkPurple, Self::Amber, Self::DyeDarkGreen, Self::DyeDarkRed, Self::DyeDarkYellow, Self::DyeBotched, Self::DyeWinterBlue, Self::DyeWinterGreen, Self::DyeWinterSilver, Self::DyeSpringBlue, Self::DyeSpringPurple, Self::DyeSpringBlack];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Undef", "AquaBlue", "Blue", "BluePurple", "Brown", "DarkBlue", "DeepBrown", "DeepGreen", "Green", "Grey", "LightBlue", "Maroon", "Navy", "Purple", "Red", "RedPurple", "Rose", "Yellow", "YellowBrown", "Copper", "Silver", "Gold", "Aqua", "DarkAquaMetal", "DarkBlueMetal", "DarkCopperMetal", "DarkGoldMetal", "DarkGreenMetal", "DarkPurpleMetal", "DarkRedMetal", "DarkSilverMetal", "LightAquaMetal", "LightBlueMetal", "LightCopperMetal", "LightGoldMetal", "LightGreenMetal", "LightPurpleMetal", "LightRedMetal", "LightSilverMetal", "Black", "Bronze", "SandyYellow", "DarkBrown", "LightBrown", "TanRed", "PaleGreen", "Tan", "PastyYellow", "SnowyWhite", "RuddyYellow", "RuddierYellow", "MidGrey", "DarkGrey", "BlueDullSilver", "YellowPaleSilver", "BrownBlueDark", "BrownBlueMed", "GreenSilver", "BrownGreen", "YellowGreen", "PalePurple", "White", "RedBrown", "GreenBrown", "OrangeBrown", "PaleGreenBrown", "PaleOrange", "GreenSlime", "BlueSlime", "YellowSlime", "PurpleSlime", "DullRed", "GreyWhite", "MediumGrey", "DullGreen", "OliveGreen", "Orange", "BlueGreen", "Olive", "Lead", "Iron", "LiteGreen", "PinkPurple", "Amber", "DyeDarkGreen", "DyeDarkRed", "DyeDarkYellow", "DyeBotched", "DyeWinterBlue", "DyeWinterGreen", "DyeWinterSilver", "DyeSpringBlue", "DyeSpringPurple", "DyeSpringBlack"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[83, 22, 1, 39, 2, 53, 77, 3, 68, 40, 4, 55, 56, 58, 19, 23, 5, 24, 42, 25, 26, 27, 52, 28, 29, 30, 6, 7, 74, 71, 87, 84, 85, 86, 93, 91, 92, 88, 89, 90, 21, 8, 63, 57, 67, 9, 72, 80, 79, 31, 10, 32, 43, 33, 34, 35, 36, 37, 38, 81, 11, 73, 51, 12, 78, 75, 76, 64, 45, 65, 66, 60, 47, 82, 13, 70, 14, 62, 15, 16, 50, 49, 41, 20, 48, 46, 44, 0, 61, 17, 18, 59, 54, 69];
}

super::support::ace_enum!(PaletteTemplate, i32, plain);
super::support::ace_enum_from!(PaletteTemplate, i32 => i64);

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/CreatureType.cs
// @generated from ACE's `Source/ACE.Entity/Enum/CreatureType.cs`; do not edit by hand

/// ACE enum `CreatureType`, underlying `uint`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct CreatureType(pub u32);

#[allow(non_upper_case_globals)]
impl CreatureType {
    pub const Invalid: Self = Self(0);
    pub const Olthoi: Self = Self(1);
    pub const Banderling: Self = Self(2);
    pub const Drudge: Self = Self(3);
    pub const Mosswart: Self = Self(4);
    pub const Lugian: Self = Self(5);
    pub const Tumerok: Self = Self(6);
    pub const Mite: Self = Self(7);
    pub const Tusker: Self = Self(8);
    pub const PhyntosWasp: Self = Self(9);
    pub const Rat: Self = Self(10);
    pub const Auroch: Self = Self(11);
    pub const Cow: Self = Self(12);
    pub const Golem: Self = Self(13);
    pub const Undead: Self = Self(14);
    pub const Gromnie: Self = Self(15);
    pub const Reedshark: Self = Self(16);
    pub const Armoredillo: Self = Self(17);
    pub const Fae: Self = Self(18);
    pub const Virindi: Self = Self(19);
    pub const Wisp: Self = Self(20);
    pub const Knathtead: Self = Self(21);
    pub const Shadow: Self = Self(22);
    pub const Mattekar: Self = Self(23);
    pub const Mumiyah: Self = Self(24);
    pub const Rabbit: Self = Self(25);
    pub const Sclavus: Self = Self(26);
    pub const ShallowsShark: Self = Self(27);
    pub const Monouga: Self = Self(28);
    pub const Zefir: Self = Self(29);
    pub const Skeleton: Self = Self(30);
    pub const Human: Self = Self(31);
    pub const Shreth: Self = Self(32);
    pub const Chittick: Self = Self(33);
    pub const Moarsman: Self = Self(34);
    pub const OlthoiLarvae: Self = Self(35);
    pub const Slithis: Self = Self(36);
    pub const Deru: Self = Self(37);
    pub const FireElemental: Self = Self(38);
    pub const Snowman: Self = Self(39);
    pub const Unknown: Self = Self(40);
    pub const Bunny: Self = Self(41);
    pub const LightningElemental: Self = Self(42);
    pub const Rockslide: Self = Self(43);
    pub const Grievver: Self = Self(44);
    pub const Niffis: Self = Self(45);
    pub const Ursuin: Self = Self(46);
    pub const Crystal: Self = Self(47);
    pub const HollowMinion: Self = Self(48);
    pub const Scarecrow: Self = Self(49);
    pub const Idol: Self = Self(50);
    pub const Empyrean: Self = Self(51);
    pub const Hopeslayer: Self = Self(52);
    pub const Doll: Self = Self(53);
    pub const Marionette: Self = Self(54);
    pub const Carenzi: Self = Self(55);
    pub const Siraluun: Self = Self(56);
    pub const AunTumerok: Self = Self(57);
    pub const HeaTumerok: Self = Self(58);
    pub const Simulacrum: Self = Self(59);
    pub const AcidElemental: Self = Self(60);
    pub const FrostElemental: Self = Self(61);
    pub const Elemental: Self = Self(62);
    pub const Statue: Self = Self(63);
    pub const Wall: Self = Self(64);
    pub const AlteredHuman: Self = Self(65);
    pub const Device: Self = Self(66);
    pub const Harbinger: Self = Self(67);
    pub const DarkSarcophagus: Self = Self(68);
    pub const Chicken: Self = Self(69);
    pub const GotrokLugian: Self = Self(70);
    pub const Margul: Self = Self(71);
    pub const BleachedRabbit: Self = Self(72);
    pub const NastyRabbit: Self = Self(73);
    pub const GrimacingRabbit: Self = Self(74);
    pub const Burun: Self = Self(75);
    pub const Target: Self = Self(76);
    pub const Ghost: Self = Self(77);
    pub const Fiun: Self = Self(78);
    pub const Eater: Self = Self(79);
    pub const Penguin: Self = Self(80);
    pub const Ruschk: Self = Self(81);
    pub const Thrungus: Self = Self(82);
    pub const ViamontianKnight: Self = Self(83);
    pub const Remoran: Self = Self(84);
    pub const Swarm: Self = Self(85);
    pub const Moar: Self = Self(86);
    pub const EnchantedArms: Self = Self(87);
    pub const Sleech: Self = Self(88);
    pub const Mukkir: Self = Self(89);
    pub const Merwart: Self = Self(90);
    pub const Food: Self = Self(91);
    pub const ParadoxOlthoi: Self = Self(92);
    pub const Harvest: Self = Self(93);
    pub const Energy: Self = Self(94);
    pub const Apparition: Self = Self(95);
    pub const Aerbax: Self = Self(96);
    pub const Touched: Self = Self(97);
    pub const BlightedMoarsman: Self = Self(98);
    pub const GearKnight: Self = Self(99);
    pub const Gurog: Self = Self(100);
    pub const Anekshay: Self = Self(101);
}

impl CreatureType {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Olthoi, Self::Banderling, Self::Drudge, Self::Mosswart, Self::Lugian, Self::Tumerok, Self::Mite, Self::Tusker, Self::PhyntosWasp, Self::Rat, Self::Auroch, Self::Cow, Self::Golem, Self::Undead, Self::Gromnie, Self::Reedshark, Self::Armoredillo, Self::Fae, Self::Virindi, Self::Wisp, Self::Knathtead, Self::Shadow, Self::Mattekar, Self::Mumiyah, Self::Rabbit, Self::Sclavus, Self::ShallowsShark, Self::Monouga, Self::Zefir, Self::Skeleton, Self::Human, Self::Shreth, Self::Chittick, Self::Moarsman, Self::OlthoiLarvae, Self::Slithis, Self::Deru, Self::FireElemental, Self::Snowman, Self::Unknown, Self::Bunny, Self::LightningElemental, Self::Rockslide, Self::Grievver, Self::Niffis, Self::Ursuin, Self::Crystal, Self::HollowMinion, Self::Scarecrow, Self::Idol, Self::Empyrean, Self::Hopeslayer, Self::Doll, Self::Marionette, Self::Carenzi, Self::Siraluun, Self::AunTumerok, Self::HeaTumerok, Self::Simulacrum, Self::AcidElemental, Self::FrostElemental, Self::Elemental, Self::Statue, Self::Wall, Self::AlteredHuman, Self::Device, Self::Harbinger, Self::DarkSarcophagus, Self::Chicken, Self::GotrokLugian, Self::Margul, Self::BleachedRabbit, Self::NastyRabbit, Self::GrimacingRabbit, Self::Burun, Self::Target, Self::Ghost, Self::Fiun, Self::Eater, Self::Penguin, Self::Ruschk, Self::Thrungus, Self::ViamontianKnight, Self::Remoran, Self::Swarm, Self::Moar, Self::EnchantedArms, Self::Sleech, Self::Mukkir, Self::Merwart, Self::Food, Self::ParadoxOlthoi, Self::Harvest, Self::Energy, Self::Apparition, Self::Aerbax, Self::Touched, Self::BlightedMoarsman, Self::GearKnight, Self::Gurog, Self::Anekshay];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Olthoi", "Banderling", "Drudge", "Mosswart", "Lugian", "Tumerok", "Mite", "Tusker", "PhyntosWasp", "Rat", "Auroch", "Cow", "Golem", "Undead", "Gromnie", "Reedshark", "Armoredillo", "Fae", "Virindi", "Wisp", "Knathtead", "Shadow", "Mattekar", "Mumiyah", "Rabbit", "Sclavus", "ShallowsShark", "Monouga", "Zefir", "Skeleton", "Human", "Shreth", "Chittick", "Moarsman", "OlthoiLarvae", "Slithis", "Deru", "FireElemental", "Snowman", "Unknown", "Bunny", "LightningElemental", "Rockslide", "Grievver", "Niffis", "Ursuin", "Crystal", "HollowMinion", "Scarecrow", "Idol", "Empyrean", "Hopeslayer", "Doll", "Marionette", "Carenzi", "Siraluun", "AunTumerok", "HeaTumerok", "Simulacrum", "AcidElemental", "FrostElemental", "Elemental", "Statue", "Wall", "AlteredHuman", "Device", "Harbinger", "DarkSarcophagus", "Chicken", "GotrokLugian", "Margul", "BleachedRabbit", "NastyRabbit", "GrimacingRabbit", "Burun", "Target", "Ghost", "Fiun", "Eater", "Penguin", "Ruschk", "Thrungus", "ViamontianKnight", "Remoran", "Swarm", "Moar", "EnchantedArms", "Sleech", "Mukkir", "Merwart", "Food", "ParadoxOlthoi", "Harvest", "Energy", "Apparition", "Aerbax", "Touched", "BlightedMoarsman", "GearKnight", "Gurog", "Anekshay"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[60, 96, 65, 101, 95, 17, 57, 11, 2, 72, 98, 41, 75, 55, 69, 33, 12, 47, 68, 37, 66, 53, 3, 79, 62, 51, 87, 94, 18, 38, 78, 91, 61, 99, 77, 13, 70, 44, 74, 15, 100, 67, 93, 58, 48, 52, 31, 50, 0, 21, 42, 5, 71, 54, 23, 90, 7, 86, 34, 28, 4, 89, 24, 73, 45, 1, 35, 92, 80, 9, 25, 10, 16, 84, 43, 81, 49, 26, 22, 27, 32, 59, 56, 30, 88, 36, 39, 63, 85, 76, 82, 97, 6, 8, 14, 40, 46, 83, 19, 64, 20, 29];
}

super::support::ace_enum!(CreatureType, u32, plain);
super::support::ace_enum_from!(CreatureType, u32 => u64, i64);

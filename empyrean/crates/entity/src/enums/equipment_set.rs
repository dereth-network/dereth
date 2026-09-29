// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/EquipmentSet.cs
// @generated from ACE's `Source/ACE.Entity/Enum/EquipmentSet.cs`; do not edit by hand

/// ACE enum `EquipmentSet`, underlying `int`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(transparent)]
pub struct EquipmentSet(pub i32);

#[allow(non_upper_case_globals)]
impl EquipmentSet {
    pub const Invalid: Self = Self(0);
    pub const Test: Self = Self(1);
    pub const Test2: Self = Self(2);
    pub const Unknown3: Self = Self(3);
    pub const CarraidasBenediction: Self = Self(4);
    pub const NobleRelic: Self = Self(5);
    pub const AncientRelic: Self = Self(6);
    pub const AlduressaRelic: Self = Self(7);
    pub const Ninja: Self = Self(8);
    pub const EmpyreanRings: Self = Self(9);
    pub const ArmMindHeart: Self = Self(10);
    pub const ArmorPerfectLight: Self = Self(11);
    pub const ArmorPerfectLight2: Self = Self(12);
    pub const Soldiers: Self = Self(13);
    pub const Adepts: Self = Self(14);
    pub const Archers: Self = Self(15);
    pub const Defenders: Self = Self(16);
    pub const Tinkers: Self = Self(17);
    pub const Crafters: Self = Self(18);
    pub const Hearty: Self = Self(19);
    pub const Dexterous: Self = Self(20);
    pub const Wise: Self = Self(21);
    pub const Swift: Self = Self(22);
    pub const Hardened: Self = Self(23);
    pub const Reinforced: Self = Self(24);
    pub const Interlocking: Self = Self(25);
    pub const Flameproof: Self = Self(26);
    pub const Acidproof: Self = Self(27);
    pub const Coldproof: Self = Self(28);
    pub const Lightningproof: Self = Self(29);
    pub const SocietyArmor: Self = Self(30);
    pub const ColosseumClothing: Self = Self(31);
    pub const GraveyardClothing: Self = Self(32);
    pub const OlthoiClothing: Self = Self(33);
    pub const NoobieArmor: Self = Self(34);
    pub const AetheriaDefense: Self = Self(35);
    pub const AetheriaDestruction: Self = Self(36);
    pub const AetheriaFury: Self = Self(37);
    pub const AetheriaGrowth: Self = Self(38);
    pub const AetheriaVigor: Self = Self(39);
    pub const RareDamageResistance: Self = Self(40);
    pub const RareDamageBoost: Self = Self(41);
    pub const OlthoiArmorDRed: Self = Self(42);
    pub const OlthoiArmorCRat: Self = Self(43);
    pub const OlthoiArmorCRed: Self = Self(44);
    pub const OlthoiArmorDRat: Self = Self(45);
    pub const AlduressaRelicUpgrade: Self = Self(46);
    pub const AncientRelicUpgrade: Self = Self(47);
    pub const NobleRelicUpgrade: Self = Self(48);
    pub const CloakAlchemy: Self = Self(49);
    pub const CloakArcaneLore: Self = Self(50);
    pub const CloakArmorTinkering: Self = Self(51);
    pub const CloakAssessPerson: Self = Self(52);
    pub const CloakLightWeapons: Self = Self(53);
    pub const CloakMissileWeapons: Self = Self(54);
    pub const CloakCooking: Self = Self(55);
    pub const CloakCreatureEnchantment: Self = Self(56);
    pub const CloakCrossbow: Self = Self(57);
    pub const CloakFinesseWeapons: Self = Self(58);
    pub const CloakDeception: Self = Self(59);
    pub const CloakFletching: Self = Self(60);
    pub const CloakHealing: Self = Self(61);
    pub const CloakItemEnchantment: Self = Self(62);
    pub const CloakItemTinkering: Self = Self(63);
    pub const CloakLeadership: Self = Self(64);
    pub const CloakLifeMagic: Self = Self(65);
    pub const CloakLoyalty: Self = Self(66);
    pub const CloakMace: Self = Self(67);
    pub const CloakMagicDefense: Self = Self(68);
    pub const CloakMagicItemTinkering: Self = Self(69);
    pub const CloakManaConversion: Self = Self(70);
    pub const CloakMeleeDefense: Self = Self(71);
    pub const CloakMissileDefense: Self = Self(72);
    pub const CloakSalvaging: Self = Self(73);
    pub const CloakSpear: Self = Self(74);
    pub const CloakStaff: Self = Self(75);
    pub const CloakHeavyWeapons: Self = Self(76);
    pub const CloakThrownWeapon: Self = Self(77);
    pub const CloakTwoHandedCombat: Self = Self(78);
    pub const CloakUnarmedCombat: Self = Self(79);
    pub const CloakVoidMagic: Self = Self(80);
    pub const CloakWarMagic: Self = Self(81);
    pub const CloakWeaponTinkering: Self = Self(82);
    pub const CloakAssessCreature: Self = Self(83);
    pub const CloakDirtyFighting: Self = Self(84);
    pub const CloakDualWield: Self = Self(85);
    pub const CloakRecklessness: Self = Self(86);
    pub const CloakShield: Self = Self(87);
    pub const CloakSneakAttack: Self = Self(88);
    pub const Ninja_New: Self = Self(89);
    pub const CloakSummoning: Self = Self(90);
    pub const ShroudedSoul: Self = Self(91);
    pub const DarkenedMind: Self = Self(92);
    pub const CloudedSpirit: Self = Self(93);
    pub const MinorStingingShroudedSoul: Self = Self(94);
    pub const MinorSparkingShroudedSoul: Self = Self(95);
    pub const MinorSmolderingShroudedSoul: Self = Self(96);
    pub const MinorShiveringShroudedSoul: Self = Self(97);
    pub const MinorStingingDarkenedMind: Self = Self(98);
    pub const MinorSparkingDarkenedMind: Self = Self(99);
    pub const MinorSmolderingDarkenedMind: Self = Self(100);
    pub const MinorShiveringDarkenedMind: Self = Self(101);
    pub const MinorStingingCloudedSpirit: Self = Self(102);
    pub const MinorSparkingCloudedSpirit: Self = Self(103);
    pub const MinorSmolderingCloudedSpirit: Self = Self(104);
    pub const MinorShiveringCloudedSpirit: Self = Self(105);
    pub const MajorStingingShroudedSoul: Self = Self(106);
    pub const MajorSparkingShroudedSoul: Self = Self(107);
    pub const MajorSmolderingShroudedSoul: Self = Self(108);
    pub const MajorShiveringShroudedSoul: Self = Self(109);
    pub const MajorStingingDarkenedMind: Self = Self(110);
    pub const MajorSparkingDarkenedMind: Self = Self(111);
    pub const MajorSmolderingDarkenedMind: Self = Self(112);
    pub const MajorShiveringDarkenedMind: Self = Self(113);
    pub const MajorStingingCloudedSpirit: Self = Self(114);
    pub const MajorSparkingCloudedSpirit: Self = Self(115);
    pub const MajorSmolderingCloudedSpirit: Self = Self(116);
    pub const MajorShiveringCloudedSpirit: Self = Self(117);
    pub const BlackfireStingingShroudedSoul: Self = Self(118);
    pub const BlackfireSparkingShroudedSoul: Self = Self(119);
    pub const BlackfireSmolderingShroudedSoul: Self = Self(120);
    pub const BlackfireShiveringShroudedSoul: Self = Self(121);
    pub const BlackfireStingingDarkenedMind: Self = Self(122);
    pub const BlackfireSparkingDarkenedMind: Self = Self(123);
    pub const BlackfireSmolderingDarkenedMind: Self = Self(124);
    pub const BlackfireShiveringDarkenedMind: Self = Self(125);
    pub const BlackfireStingingCloudedSpirit: Self = Self(126);
    pub const BlackfireSparkingCloudedSpirit: Self = Self(127);
    pub const BlackfireSmolderingCloudedSpirit: Self = Self(128);
    pub const BlackfireShiveringCloudedSpirit: Self = Self(129);
    pub const ShimmeringShadowsSet: Self = Self(130);
    pub const BrownSocietyLocket: Self = Self(131);
    pub const YellowSocietyLocket: Self = Self(132);
    pub const RedSocietyBand: Self = Self(133);
    pub const GreenSocietyBand: Self = Self(134);
    pub const PurpleSocietyBand: Self = Self(135);
    pub const BlueSocietyBand: Self = Self(136);
    pub const GauntletGarb: Self = Self(137);
    pub const ParagonMissile: Self = Self(138);
    pub const ParagonCaster: Self = Self(139);
    pub const ParagonMelee: Self = Self(140);
}

impl EquipmentSet {
    /// Every declared member, aliases included, in .NET `Enum.GetValues` order.
    pub const ALL: &'static [Self] = &[Self::Invalid, Self::Test, Self::Test2, Self::Unknown3, Self::CarraidasBenediction, Self::NobleRelic, Self::AncientRelic, Self::AlduressaRelic, Self::Ninja, Self::EmpyreanRings, Self::ArmMindHeart, Self::ArmorPerfectLight, Self::ArmorPerfectLight2, Self::Soldiers, Self::Adepts, Self::Archers, Self::Defenders, Self::Tinkers, Self::Crafters, Self::Hearty, Self::Dexterous, Self::Wise, Self::Swift, Self::Hardened, Self::Reinforced, Self::Interlocking, Self::Flameproof, Self::Acidproof, Self::Coldproof, Self::Lightningproof, Self::SocietyArmor, Self::ColosseumClothing, Self::GraveyardClothing, Self::OlthoiClothing, Self::NoobieArmor, Self::AetheriaDefense, Self::AetheriaDestruction, Self::AetheriaFury, Self::AetheriaGrowth, Self::AetheriaVigor, Self::RareDamageResistance, Self::RareDamageBoost, Self::OlthoiArmorDRed, Self::OlthoiArmorCRat, Self::OlthoiArmorCRed, Self::OlthoiArmorDRat, Self::AlduressaRelicUpgrade, Self::AncientRelicUpgrade, Self::NobleRelicUpgrade, Self::CloakAlchemy, Self::CloakArcaneLore, Self::CloakArmorTinkering, Self::CloakAssessPerson, Self::CloakLightWeapons, Self::CloakMissileWeapons, Self::CloakCooking, Self::CloakCreatureEnchantment, Self::CloakCrossbow, Self::CloakFinesseWeapons, Self::CloakDeception, Self::CloakFletching, Self::CloakHealing, Self::CloakItemEnchantment, Self::CloakItemTinkering, Self::CloakLeadership, Self::CloakLifeMagic, Self::CloakLoyalty, Self::CloakMace, Self::CloakMagicDefense, Self::CloakMagicItemTinkering, Self::CloakManaConversion, Self::CloakMeleeDefense, Self::CloakMissileDefense, Self::CloakSalvaging, Self::CloakSpear, Self::CloakStaff, Self::CloakHeavyWeapons, Self::CloakThrownWeapon, Self::CloakTwoHandedCombat, Self::CloakUnarmedCombat, Self::CloakVoidMagic, Self::CloakWarMagic, Self::CloakWeaponTinkering, Self::CloakAssessCreature, Self::CloakDirtyFighting, Self::CloakDualWield, Self::CloakRecklessness, Self::CloakShield, Self::CloakSneakAttack, Self::Ninja_New, Self::CloakSummoning, Self::ShroudedSoul, Self::DarkenedMind, Self::CloudedSpirit, Self::MinorStingingShroudedSoul, Self::MinorSparkingShroudedSoul, Self::MinorSmolderingShroudedSoul, Self::MinorShiveringShroudedSoul, Self::MinorStingingDarkenedMind, Self::MinorSparkingDarkenedMind, Self::MinorSmolderingDarkenedMind, Self::MinorShiveringDarkenedMind, Self::MinorStingingCloudedSpirit, Self::MinorSparkingCloudedSpirit, Self::MinorSmolderingCloudedSpirit, Self::MinorShiveringCloudedSpirit, Self::MajorStingingShroudedSoul, Self::MajorSparkingShroudedSoul, Self::MajorSmolderingShroudedSoul, Self::MajorShiveringShroudedSoul, Self::MajorStingingDarkenedMind, Self::MajorSparkingDarkenedMind, Self::MajorSmolderingDarkenedMind, Self::MajorShiveringDarkenedMind, Self::MajorStingingCloudedSpirit, Self::MajorSparkingCloudedSpirit, Self::MajorSmolderingCloudedSpirit, Self::MajorShiveringCloudedSpirit, Self::BlackfireStingingShroudedSoul, Self::BlackfireSparkingShroudedSoul, Self::BlackfireSmolderingShroudedSoul, Self::BlackfireShiveringShroudedSoul, Self::BlackfireStingingDarkenedMind, Self::BlackfireSparkingDarkenedMind, Self::BlackfireSmolderingDarkenedMind, Self::BlackfireShiveringDarkenedMind, Self::BlackfireStingingCloudedSpirit, Self::BlackfireSparkingCloudedSpirit, Self::BlackfireSmolderingCloudedSpirit, Self::BlackfireShiveringCloudedSpirit, Self::ShimmeringShadowsSet, Self::BrownSocietyLocket, Self::YellowSocietyLocket, Self::RedSocietyBand, Self::GreenSocietyBand, Self::PurpleSocietyBand, Self::BlueSocietyBand, Self::GauntletGarb, Self::ParagonMissile, Self::ParagonCaster, Self::ParagonMelee];
    /// The name of each entry of [`Self::ALL`], in the same order.
    pub const NAMES: &'static [&'static str] = &["Invalid", "Test", "Test2", "Unknown3", "CarraidasBenediction", "NobleRelic", "AncientRelic", "AlduressaRelic", "Ninja", "EmpyreanRings", "ArmMindHeart", "ArmorPerfectLight", "ArmorPerfectLight2", "Soldiers", "Adepts", "Archers", "Defenders", "Tinkers", "Crafters", "Hearty", "Dexterous", "Wise", "Swift", "Hardened", "Reinforced", "Interlocking", "Flameproof", "Acidproof", "Coldproof", "Lightningproof", "SocietyArmor", "ColosseumClothing", "GraveyardClothing", "OlthoiClothing", "NoobieArmor", "AetheriaDefense", "AetheriaDestruction", "AetheriaFury", "AetheriaGrowth", "AetheriaVigor", "RareDamageResistance", "RareDamageBoost", "OlthoiArmorDRed", "OlthoiArmorCRat", "OlthoiArmorCRed", "OlthoiArmorDRat", "AlduressaRelicUpgrade", "AncientRelicUpgrade", "NobleRelicUpgrade", "CloakAlchemy", "CloakArcaneLore", "CloakArmorTinkering", "CloakAssessPerson", "CloakLightWeapons", "CloakMissileWeapons", "CloakCooking", "CloakCreatureEnchantment", "CloakCrossbow", "CloakFinesseWeapons", "CloakDeception", "CloakFletching", "CloakHealing", "CloakItemEnchantment", "CloakItemTinkering", "CloakLeadership", "CloakLifeMagic", "CloakLoyalty", "CloakMace", "CloakMagicDefense", "CloakMagicItemTinkering", "CloakManaConversion", "CloakMeleeDefense", "CloakMissileDefense", "CloakSalvaging", "CloakSpear", "CloakStaff", "CloakHeavyWeapons", "CloakThrownWeapon", "CloakTwoHandedCombat", "CloakUnarmedCombat", "CloakVoidMagic", "CloakWarMagic", "CloakWeaponTinkering", "CloakAssessCreature", "CloakDirtyFighting", "CloakDualWield", "CloakRecklessness", "CloakShield", "CloakSneakAttack", "Ninja_New", "CloakSummoning", "ShroudedSoul", "DarkenedMind", "CloudedSpirit", "MinorStingingShroudedSoul", "MinorSparkingShroudedSoul", "MinorSmolderingShroudedSoul", "MinorShiveringShroudedSoul", "MinorStingingDarkenedMind", "MinorSparkingDarkenedMind", "MinorSmolderingDarkenedMind", "MinorShiveringDarkenedMind", "MinorStingingCloudedSpirit", "MinorSparkingCloudedSpirit", "MinorSmolderingCloudedSpirit", "MinorShiveringCloudedSpirit", "MajorStingingShroudedSoul", "MajorSparkingShroudedSoul", "MajorSmolderingShroudedSoul", "MajorShiveringShroudedSoul", "MajorStingingDarkenedMind", "MajorSparkingDarkenedMind", "MajorSmolderingDarkenedMind", "MajorShiveringDarkenedMind", "MajorStingingCloudedSpirit", "MajorSparkingCloudedSpirit", "MajorSmolderingCloudedSpirit", "MajorShiveringCloudedSpirit", "BlackfireStingingShroudedSoul", "BlackfireSparkingShroudedSoul", "BlackfireSmolderingShroudedSoul", "BlackfireShiveringShroudedSoul", "BlackfireStingingDarkenedMind", "BlackfireSparkingDarkenedMind", "BlackfireSmolderingDarkenedMind", "BlackfireShiveringDarkenedMind", "BlackfireStingingCloudedSpirit", "BlackfireSparkingCloudedSpirit", "BlackfireSmolderingCloudedSpirit", "BlackfireShiveringCloudedSpirit", "ShimmeringShadowsSet", "BrownSocietyLocket", "YellowSocietyLocket", "RedSocietyBand", "GreenSocietyBand", "PurpleSocietyBand", "BlueSocietyBand", "GauntletGarb", "ParagonMissile", "ParagonCaster", "ParagonMelee"];
    /// Indices into [`Self::NAMES`], sorted by ordinal name.
    const NAME_ORDER: &'static [u16] = &[27, 14, 35, 36, 37, 38, 39, 7, 46, 6, 47, 15, 10, 11, 12, 129, 125, 121, 128, 124, 120, 127, 123, 119, 126, 122, 118, 136, 131, 4, 49, 50, 51, 83, 52, 55, 56, 57, 59, 84, 85, 58, 60, 61, 76, 62, 63, 64, 65, 53, 66, 67, 68, 69, 70, 71, 72, 54, 86, 73, 87, 88, 74, 75, 90, 77, 78, 79, 80, 81, 82, 93, 28, 31, 18, 92, 16, 20, 9, 26, 137, 32, 134, 23, 19, 25, 0, 29, 117, 113, 109, 116, 112, 108, 115, 111, 107, 114, 110, 106, 105, 101, 97, 104, 100, 96, 103, 99, 95, 102, 98, 94, 8, 89, 5, 48, 34, 43, 44, 45, 42, 33, 139, 140, 138, 135, 41, 40, 133, 24, 130, 91, 30, 13, 22, 1, 2, 17, 3, 21, 132];
}

super::support::ace_enum!(EquipmentSet, i32, plain);
super::support::ace_enum_from!(EquipmentSet, i32 => i64);

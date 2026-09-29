// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/ArmorTypeChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/ArmorTypeChance.cs`; do not edit by hand

//! The literal data of ACE's `ArmorTypeChance` (`Factories/Tables/ArmorTypeChance.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureArmorType;

/// ACE `ArmorTypeChance.T1_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T1_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.34),
    (TreasureArmorType::StuddedLeather, 0.33),
    (TreasureArmorType::Chainmail, 0.33),
]);

/// ACE `ArmorTypeChance.T2_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T2_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.25),
    (TreasureArmorType::StuddedLeather, 0.25),
    (TreasureArmorType::Chainmail, 0.25),
    (TreasureArmorType::Platemail, 0.25),
]);

/// ACE `ArmorTypeChance.T3_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T3_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.22),
    (TreasureArmorType::StuddedLeather, 0.22),
    (TreasureArmorType::Chainmail, 0.22),
    (TreasureArmorType::Platemail, 0.22),
    (TreasureArmorType::HeritageLow, 0.05),
    (TreasureArmorType::Covenant, 0.05),
    (TreasureArmorType::Overrobe, 0.02),
]);

/// ACE `ArmorTypeChance.T4_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T4_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.16),
    (TreasureArmorType::StuddedLeather, 0.16),
    (TreasureArmorType::Chainmail, 0.17),
    (TreasureArmorType::Platemail, 0.17),
    (TreasureArmorType::HeritageLow, 0.16),
    (TreasureArmorType::Covenant, 0.16),
    (TreasureArmorType::Overrobe, 0.02),
]);

/// ACE `ArmorTypeChance.T5_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T5_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.15),
    (TreasureArmorType::StuddedLeather, 0.15),
    (TreasureArmorType::Chainmail, 0.16),
    (TreasureArmorType::Platemail, 0.16),
    (TreasureArmorType::HeritageLow, 0.15),
    (TreasureArmorType::Covenant, 0.16),
    (TreasureArmorType::HeritageHigh, 0.05),
    (TreasureArmorType::Overrobe, 0.02),
]);

/// ACE `ArmorTypeChance.T6_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T6_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.12),
    (TreasureArmorType::StuddedLeather, 0.12),
    (TreasureArmorType::Chainmail, 0.12),
    (TreasureArmorType::Platemail, 0.12),
    (TreasureArmorType::HeritageLow, 0.12),
    (TreasureArmorType::Covenant, 0.15),
    (TreasureArmorType::HeritageHigh, 0.15),
    (TreasureArmorType::Haebrean, 0.04),
    (TreasureArmorType::KnorrAcademy, 0.02),
    (TreasureArmorType::Sedgemail, 0.02),
    (TreasureArmorType::Overrobe, 0.02),
]);

/// ACE `ArmorTypeChance.T7_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T7_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.11),
    (TreasureArmorType::StuddedLeather, 0.11),
    (TreasureArmorType::Chainmail, 0.12),
    (TreasureArmorType::Platemail, 0.12),
    (TreasureArmorType::HeritageLow, 0.12),
    (TreasureArmorType::Covenant, 0.12),
    (TreasureArmorType::HeritageHigh, 0.12),
    (TreasureArmorType::Olthoi, 0.03),
    (TreasureArmorType::OlthoiHeritage, 0.05),
    (TreasureArmorType::Haebrean, 0.04),
    (TreasureArmorType::KnorrAcademy, 0.02),
    (TreasureArmorType::Sedgemail, 0.02),
    (TreasureArmorType::Overrobe, 0.02),
]);

/// ACE `ArmorTypeChance.T8_Chances` (`ChanceTable<TreasureArmorType>`).
pub static T8_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.10),
    (TreasureArmorType::StuddedLeather, 0.10),
    (TreasureArmorType::Chainmail, 0.10),
    (TreasureArmorType::Platemail, 0.10),
    (TreasureArmorType::HeritageLow, 0.11),
    (TreasureArmorType::Covenant, 0.07),
    (TreasureArmorType::HeritageHigh, 0.14),
    (TreasureArmorType::Olthoi, 0.06),
    (TreasureArmorType::OlthoiHeritage, 0.12),
    (TreasureArmorType::Haebrean, 0.04),
    (TreasureArmorType::KnorrAcademy, 0.02),
    (TreasureArmorType::Sedgemail, 0.02),
    (TreasureArmorType::Overrobe, 0.02),
]);

/// ACE `ArmorTypeChance.armorTiers` (`List<ChanceTable<TreasureArmorType>>`).
pub static ARMOR_TIERS: [&ChanceTable<TreasureArmorType>; 8] = [
    &T1_CHANCES,
    &T2_CHANCES,
    &T3_CHANCES,
    &T4_CHANCES,
    &T5_CHANCES,
    &T6_CHANCES,
    &T7_CHANCES,
    &T8_CHANCES,
];

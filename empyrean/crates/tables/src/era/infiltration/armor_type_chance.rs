// Rules ported from ClassicACE (bDekaru), AGPL-3.0: Source/ACE.Server/Factories/Tables/ArmorTypeChance.cs
// @generated from ClassicACE's `Source/ACE.Server/Factories/Tables/ArmorTypeChance.cs`; do not edit by hand

//! The tables of ClassicACE's `ArmorTypeChance` under its Infiltration ruleset
//! (`Factories/Tables/ArmorTypeChance.cs`).

use crate::entity::ChanceTable;
use crate::enums::TreasureArmorType;

/// ClassicACE `ArmorTypeChance.T1_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T1_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.34),
    (TreasureArmorType::StuddedLeather, 0.33),
    (TreasureArmorType::Chainmail, 0.33),
]);

/// ClassicACE `ArmorTypeChance.T2_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T2_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.25),
    (TreasureArmorType::StuddedLeather, 0.25),
    (TreasureArmorType::Chainmail, 0.25),
    (TreasureArmorType::Platemail, 0.25),
]);

/// ClassicACE `ArmorTypeChance.T3_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T3_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.22),
    (TreasureArmorType::StuddedLeather, 0.22),
    (TreasureArmorType::Chainmail, 0.22),
    (TreasureArmorType::Platemail, 0.22),
    (TreasureArmorType::HeritageLow, 0.06),
    (TreasureArmorType::Covenant, 0.06),
]);

/// ClassicACE `ArmorTypeChance.T4_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T4_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.16),
    (TreasureArmorType::StuddedLeather, 0.16),
    (TreasureArmorType::Chainmail, 0.17),
    (TreasureArmorType::Platemail, 0.17),
    (TreasureArmorType::HeritageLow, 0.17),
    (TreasureArmorType::Covenant, 0.17),
]);

/// ClassicACE `ArmorTypeChance.T5_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T5_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.15),
    (TreasureArmorType::StuddedLeather, 0.16),
    (TreasureArmorType::Chainmail, 0.16),
    (TreasureArmorType::Platemail, 0.16),
    (TreasureArmorType::HeritageLow, 0.16),
    (TreasureArmorType::Covenant, 0.16),
    (TreasureArmorType::HeritageHigh, 0.05),
]);

/// ClassicACE `ArmorTypeChance.T6_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T6_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.15),
    (TreasureArmorType::StuddedLeather, 0.15),
    (TreasureArmorType::Chainmail, 0.15),
    (TreasureArmorType::Platemail, 0.15),
    (TreasureArmorType::HeritageLow, 0.15),
    (TreasureArmorType::Covenant, 0.15),
    (TreasureArmorType::HeritageHigh, 0.10),
]);

/// ClassicACE `ArmorTypeChance.T7_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T7_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.15),
    (TreasureArmorType::StuddedLeather, 0.15),
    (TreasureArmorType::Chainmail, 0.15),
    (TreasureArmorType::Platemail, 0.15),
    (TreasureArmorType::HeritageLow, 0.15),
    (TreasureArmorType::Covenant, 0.15),
    (TreasureArmorType::HeritageHigh, 0.10),
]);

/// ClassicACE `ArmorTypeChance.T8_Chances`, as its Infiltration ruleset sets it (`ChanceTable<TreasureArmorType>`).
pub static T8_CHANCES: ChanceTable<TreasureArmorType> = ChanceTable::new(&[
    (TreasureArmorType::Leather, 0.15),
    (TreasureArmorType::StuddedLeather, 0.15),
    (TreasureArmorType::Chainmail, 0.15),
    (TreasureArmorType::Platemail, 0.15),
    (TreasureArmorType::HeritageLow, 0.15),
    (TreasureArmorType::Covenant, 0.15),
    (TreasureArmorType::HeritageHigh, 0.10),
]);

/// ClassicACE `ArmorTypeChance.armorTiers`, as its Infiltration ruleset sets it (`List<ChanceTable<TreasureArmorType>>`).
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

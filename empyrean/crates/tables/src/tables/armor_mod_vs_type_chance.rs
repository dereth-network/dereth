// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/ArmorModVsTypeChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/ArmorModVsTypeChance.cs`; do not edit by hand

//! The literal data of ACE's `ArmorModVsTypeChance` (`Factories/Tables/ArmorModVsTypeChance.cs`).

use crate::entity::ChanceTable;

/// ACE `ArmorModVsTypeChance.TierChances` (`List<float>`).
pub static TIER_CHANCES: [f32; 8] = [
    0.00,
    0.01,
    0.05,
    0.08,
    0.25,
    0.40,
    0.40,
    0.40,
];

/// ACE `ArmorModVsTypeChance.ArmorModVsType_T2_QualityLevel` (`ChanceTable<int>`).
pub static ARMOR_MOD_VS_TYPE_T2_QUALITY_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 1.0),
]);

/// ACE `ArmorModVsTypeChance.ArmorModVsType_T3_QualityLevel` (`ChanceTable<int>`).
pub static ARMOR_MOD_VS_TYPE_T3_QUALITY_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.5),
    (2, 0.5),
]);

/// ACE `ArmorModVsTypeChance.ArmorModVsType_T4_QualityLevel` (`ChanceTable<int>`).
pub static ARMOR_MOD_VS_TYPE_T4_QUALITY_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.25),
    (2, 0.50),
    (3, 0.25),
]);

/// ACE `ArmorModVsTypeChance.ArmorModVsType_T5_QualityLevel` (`ChanceTable<int>`).
pub static ARMOR_MOD_VS_TYPE_T5_QUALITY_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (2, 0.25),
    (3, 0.50),
    (4, 0.25),
]);

/// ACE `ArmorModVsTypeChance.ArmorModVsType_T6_T8_QualityLevel` (`ChanceTable<int>`).
pub static ARMOR_MOD_VS_TYPE_T6_T8_QUALITY_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (2, 0.25),
    (3, 0.35),
    (4, 0.30),
    (5, 0.10),
]);

/// ACE `ArmorModVsTypeChance.qualityLevels` (`List<ChanceTable<int>>`).
pub static QUALITY_LEVELS: [Option<&ChanceTable<i32>>; 8] = [
    None,
    Some(&ARMOR_MOD_VS_TYPE_T2_QUALITY_LEVEL),
    Some(&ARMOR_MOD_VS_TYPE_T3_QUALITY_LEVEL),
    Some(&ARMOR_MOD_VS_TYPE_T4_QUALITY_LEVEL),
    Some(&ARMOR_MOD_VS_TYPE_T5_QUALITY_LEVEL),
    Some(&ARMOR_MOD_VS_TYPE_T6_T8_QUALITY_LEVEL),
    Some(&ARMOR_MOD_VS_TYPE_T6_T8_QUALITY_LEVEL),
    Some(&ARMOR_MOD_VS_TYPE_T6_T8_QUALITY_LEVEL),
];

// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Factories/Tables/AetheriaChance.cs
// @generated from ACE's `Source/ACE.Server/Factories/Tables/AetheriaChance.cs`; do not edit by hand

//! The literal data of ACE's `AetheriaChance` (`Factories/Tables/AetheriaChance.cs`).

use crate::entity::ChanceTable;

/// ACE `AetheriaChance.T5_ItemMaxLevel` (`ChanceTable<int>`).
pub static T5_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.60),
    (2, 0.40),
]);

/// ACE `AetheriaChance.T6_ItemMaxLevel` (`ChanceTable<int>`).
pub static T6_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.550),
    (2, 0.345),
    (3, 0.100),
    (4, 0.005),
]);

/// ACE `AetheriaChance.T7_ItemMaxLevel` (`ChanceTable<int>`).
pub static T7_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.55000),
    (2, 0.34475),
    (3, 0.10000),
    (4, 0.00500),
    (5, 0.00025),
]);

/// ACE `AetheriaChance.T8_ItemMaxLevel` (`ChanceTable<int>`).
pub static T8_ITEM_MAX_LEVEL: ChanceTable<i32> = ChanceTable::new(&[
    (1, 0.0200),
    (2, 0.5500),
    (3, 0.4195),
    (4, 0.0100),
    (5, 0.0005),
]);

/// ACE `AetheriaChance.itemMaxLevels` (`List<ChanceTable<int>>`).
pub static ITEM_MAX_LEVELS: [&ChanceTable<i32>; 4] = [
    &T5_ITEM_MAX_LEVEL,
    &T6_ITEM_MAX_LEVEL,
    &T7_ITEM_MAX_LEVEL,
    &T8_ITEM_MAX_LEVEL,
];
